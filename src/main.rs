use std::{fs::{read_to_string, File}, io::Write, sync::Arc, time::Duration};

use bevy::{audio::Source, input::{keyboard::KeyboardInput, mouse::MouseMotion}, log::tracing_subscriber::{filter::targets::Iter, fmt::time}, prelude::*, render::{render_asset::RenderAssetUsages, render_resource::{Extent3d, TextureFormat}}, transform::commands, window::{PrimaryWindow, WindowMode}};
use rand::Rng;
#[derive(Component)]
struct Drawable;

#[derive(Resource)]
struct UndoHistory(Vec<Vec<Entity>>);

#[derive(Resource)]
struct StopSoundWhenNoMotionBy(Duration);

#[derive(Resource)]
struct CurrentColor(usize);

fn stop_sound_if_mouse_stopped(time:Res<Time>, stop:Option<Res<StopSoundWhenNoMotionBy>>, sound_sink:Query<&AudioSink, With<ChalkSound>>) {
    if let Some(timestamp) = stop {
        if time.elapsed() > timestamp.0 {
            if let Ok(sink) = sound_sink.get_single() {
                sink.set_volume(0.0);
            }
        }
    }
}

#[derive(Resource)]
struct FileSettings{
    eraser_radius:f32,
    mouse_speed_multiplier:f32,
    max_volume:f32,
}

fn eraser(
    windows: Query<&Window>, 
    input: Res<ButtonInput<MouseButton>>,
    splats:Query<(Entity, &Style), With<UiImage>>,
    settings:Res<FileSettings>,
    mut commands:Commands,
) {
    let window = windows.single();
    if let Some(pos) = window.cursor_position() {
        if input.pressed(MouseButton::Right) {
            for (splat, style) in &splats {
                if let Val::Px(left) = style.left {
                    if let Val::Px(top) = style.top {
                        if pos.distance(Vec2{x:left, y:top}) <= settings.eraser_radius {
                            commands.entity(splat).remove_parent().despawn();
                        }
                    }
                }
            }
        }
    }
}

fn on_mouse_move( 
    mut last_pos:Local<Vec2>,
    time:Res<Time>,
    windows: Query<&Window>, 
    brushes:Res<Brushes>,
    input: Res<ButtonInput<MouseButton>>, 
    // mut images:ResMut<Assets<Image>>,
    // query:Query<&UiImage, With<Drawable>>,
    assets:ResMut<AssetServer>,
    current_color:Res<CurrentColor>,
    sound_sink:Query<(Entity, &AudioSink), With<ChalkSound>>,
    bglayer: Query<Entity, With<BGLayer>>,
    volume_controls:Res<FileSettings>,
    mut commands:Commands,
    mut history: ResMut<UndoHistory>,
) {
    const STROKE_SIZE:f32 = 16.0;
    let bglayer = bglayer.single();
    use rand::thread_rng;
    let window = windows.single();
    if let Some(pos) = window.cursor_position() {
        if input.just_pressed(MouseButton::Left) {
            *last_pos = pos;
            commands.spawn(AudioBundle{
                source: assets.load("thump.ogg"),
                settings:PlaybackSettings::DESPAWN
            });
            commands.spawn((ChalkSound, AudioBundle{
                source: assets.load("chalk.ogg"),
                settings:PlaybackSettings::LOOP.with_volume(bevy::audio::Volume::new(0.0))
            }));
        }
        if input.just_released(MouseButton::Left) {
            if let Ok((entity, sink)) = sound_sink.get_single() {
                sink.stop();
                commands.entity(entity).despawn();
            }
            if let Some(len) = history.0.last().and_then(|v| Some(v.len())) {
                if len > 0 {
                    history.0.push(Vec::new())
                }
            }
        }
        if input.pressed(MouseButton::Left) {
            let delta = pos - *last_pos;
            if delta != Vec2::ZERO {
                if let Ok((_, sink)) = sound_sink.get_single() {
                    sink.set_volume((delta.length()*volume_controls.mouse_speed_multiplier).clamp(0.0, volume_controls.max_volume));
                    commands.insert_resource(StopSoundWhenNoMotionBy(time.elapsed() + Duration::from_millis(100)))
                }
                let mut rng = thread_rng();
                let mut start = pos - delta;
                // println!("Writing to last vector of history of size {}", history.0.len());
                if (pos - start).length() >= 0.25*STROKE_SIZE {
                    let dir = delta.normalize();
                    // println!("Start:{start:?}, dest:{pos:?}, delta:{delta:?}, dir:{dir:?}");
                    while (pos - start).length() > 0.25*STROKE_SIZE {
                        let idx = rng.gen_range(0..brushes[0].len());
                        commands.entity(bglayer).with_children(|bg| {
                            let e = bg.spawn(ImageBundle{style:Style{position_type:PositionType::Absolute, left:Val::Px(start.x-0.5*STROKE_SIZE), top:Val::Px(start.y-0.5*STROKE_SIZE),width:Val::Px(STROKE_SIZE),height:Val::Px(STROKE_SIZE),..default()}, image:UiImage{texture:brushes[current_color.0][idx].clone_weak(),..default()},..default()}).id();
                            history.0.last_mut().unwrap().push(e);
                        });
                        start += 0.25*STROKE_SIZE*dir;
                        // println!("Start:{start:?}, dest:{pos:?}, delta:{delta:?}, dir:{dir:?}");
                    }
                } else {
                    let idx = rng.gen_range(0..brushes[0].len());
                    commands.entity(bglayer).with_children(|bg| {
                        let e = bg.spawn(ImageBundle{style:Style{position_type:PositionType::Absolute, left:Val::Px(pos.x-0.5*STROKE_SIZE), top:Val::Px(pos.y-0.5*STROKE_SIZE),width:Val::Px(STROKE_SIZE),height:Val::Px(STROKE_SIZE),..default()}, image:UiImage{texture:brushes[current_color.0][idx].clone_weak(),..default()},..default()}).id();  
                        history.0.last_mut().unwrap().push(e);
                    });
                }
            }
            *last_pos = pos;
        }

    }
}

#[derive(Resource, Deref, DerefMut)]
/// Vec of colors then vec of different brushes
struct Brushes(Vec<Vec<Handle<Image>>>);

#[derive(Resource)]
struct Chalks(Vec<Handle<Image>>);

#[derive(Resource)]
struct Background(Handle<Image>);

#[derive(Component)]
struct ThumpSound;

#[derive(Component)]
struct ChalkSound;


fn set_color(mut images:ResMut<Assets<Image>>, 
    mut load_events:EventReader<AssetEvent<Image>>, 
    mut brushes:ResMut<Brushes>, 
    mut chalks:ResMut<Chalks>,
    background: Res<Background>,
    chalk_panel: Query<Entity, With<ChalksPanel>>,
    mut commands:Commands
) {
    let chalk_panel = chalk_panel.single();
    for event in load_events.read() {
        match event {
            AssetEvent::LoadedWithDependencies { id } => {
                if background.0.id() != *id { // Don't want to change colors to background, only to brushes and chalks
                    let mut colored_images = Vec::new();
                    if let Some(image) = images.get(*id) {
                        let pixel_size = match image.texture_descriptor.format {
                            TextureFormat::Rgba8UnormSrgb => 4,
                            f @ _ => panic!("Unexpected format {f:?}")
                        };
                        for color_idx in 0..12 {
                            let mut new_image = image.clone();
                            let mut offset = 0;
                            while offset < new_image.data.len() {
                                let mut color = Color::rgba_u8(new_image.data[offset], new_image.data[offset+1], new_image.data[offset+2], new_image.data[offset+3]).as_hsla();
                                color.set_s(1.0);
                                color.set_h(30.0*color_idx as f32);
                                let l = color.l();
                                if l > 0.5 {
                                    color.set_l(0.5*l);
                                }
                                let color_bytes = color.as_rgba_u8();
                                new_image.data[offset] = color_bytes[0];
                                new_image.data[offset+1] = color_bytes[1];
                                new_image.data[offset+2] = color_bytes[2];
                                // image.data[offset+3] = 255;//color_bytes[3];
                                offset += pixel_size;
                            }
                            colored_images.push(new_image);
                        }
                    }
                    for (color_idx, image) in colored_images.into_iter().enumerate() {
                        let handle = images.add(image);
                        if chalks.0[0].id() == *id {
                            chalks.0.push(handle.clone());
                            commands.entity(chalk_panel).with_children(|panel| _ = panel.spawn((Interaction::None, ColorChalk(color_idx+1), ImageBundle{style:Style{margin:UiRect::vertical(Val::Px(6.)),..default()},focus_policy:bevy::ui::FocusPolicy::Block,image:UiImage::new(handle),..default()})));
                        } else {
                            if let Some(vec) = brushes.get_mut(color_idx+1) {
                                vec.push(handle)
                            } else {
                                brushes.push(vec![handle])
                            }
                        }
                    }

                }
            },
            _ => (),
        }
    }
}

#[derive(Component)]
struct BGLayer;

#[derive(Component)]
struct ChalksPanel;

#[derive(Component)]
struct ColorChalk(usize);

fn on_chalk_interaction(
    mut chalks:Query<(&Interaction, &ColorChalk, &mut Style), Changed<Interaction>>,
    mut current_color: ResMut<CurrentColor>,
) {
    for (interaction, color, mut style) in &mut chalks {
        match interaction {
            Interaction::Hovered => {
                style.left = Val::Px(16.0);
            },
            Interaction::None => {
                style.left = Val::Px(0.0);
            },
            Interaction::Pressed => {
                current_color.0 = color.0;
            }
        }
    }
}

fn fullscreen(keyboard:Res<ButtonInput<KeyCode>>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
) {
    if keyboard.just_pressed(KeyCode::KeyF) {
        let mut window = window.single_mut();
        match window.mode {
            WindowMode::Windowed => window.mode = WindowMode::BorderlessFullscreen,
            _ => window.mode = WindowMode::Windowed,
        }
        
    }
}

fn clear_all(
    mut undo_history:ResMut<UndoHistory>,
    keyboard:Res<ButtonInput<KeyCode>>,
    mut commands:Commands,
) {
    if keyboard.just_pressed(KeyCode::Backspace) {
        while let Some(mut entities) = undo_history.0.pop() {
            while let Some(e) = entities.pop() {
                if let Some(mut e) = commands.get_entity(e) {
                    e.remove_parent().despawn()
                }
            }
        }
        undo_history.0.push(Vec::new());
    }
}

fn undo(
    mut undo_history:ResMut<UndoHistory>,
    keyboard:Res<ButtonInput<KeyCode>>,
    mut commands:Commands,
) {
    if keyboard.just_pressed(KeyCode::KeyZ) && (keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight)) {
        if undo_history.0.last().unwrap().len() > 0 {
            let entities = undo_history.0.last_mut().unwrap();
            while let Some(e) = entities.pop() {
                if let Some(mut e) = commands.get_entity(e) {
                    e.remove_parent().despawn()
                }
            }
        } else {
            if let Some(entities) = undo_history.0.iter_mut().rev().skip(1).next() {
                while let Some(e) = entities.pop() {
                    if let Some(mut e) = commands.get_entity(e) {
                        e.remove_parent().despawn()
                    }
                }
                undo_history.0.pop();
            }
        }
    }
}

fn setup(mut commands:Commands, windows: Query<&Window>, assets:ResMut<AssetServer>, mut images:ResMut<Assets<Image>>) {
    let window = windows.single();
    let width = window.width() as u32;
    let height = window.height() as u32;

    commands.spawn(Camera2dBundle{camera:Camera{hdr:false,..default()},..default()});
    let background = assets.load("background.jpg");
    commands.insert_resource(Background(background.clone()));
    let image_bundle = ImageBundle{style:Style{width:Val::Percent(100.), height:Val::Percent(100.),position_type:PositionType::Absolute,..default()},image:UiImage{texture:background,..default()},..default()};
    commands.insert_resource(Brushes(vec![vec![
        assets.load("chalk.png"),
        assets.load("chalk2.png"),
        assets.load("chalk3.png"),
        assets.load("chalk4.png"),
        assets.load("chalk5.png"),
    ]]));
    let chalk = assets.load("chalk_icon.png");
    commands.insert_resource(Chalks(vec![chalk.clone()]));
    commands.spawn((BGLayer, image_bundle, ImageScaleMode::Tiled { tile_x: true, tile_y: true, stretch_value: 1.0 })).with_children(|bg| {
        bg.spawn((ChalksPanel, NodeBundle{style:Style{position_type:PositionType::Absolute, top:Val::Percent(20.), flex_direction:FlexDirection::Column, left:Val::Px(-16.0),..default()},..default()})).with_children(|panel| {
            // for idx in 0..12 {
                panel.spawn((Interaction::None, ColorChalk(0), ImageBundle{style:Style{margin:UiRect::vertical(Val::Px(6.)),..default()},image:UiImage::new(chalk),focus_policy:bevy::ui::FocusPolicy::Block,..default()}));
            // }
        });

    });
}

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins);
    let config_file_path = "chalkboard_config.txt";
    let mut mouse_speed_multiplier = 0.1;
    let mut max_volume = 1.0;
    let mut eraser_radius = 16.0;
    if let Ok(file) = read_to_string(config_file_path) {
        for line in file.lines() {
            let mut tokens = line.split('=');
            if let Some(key) = tokens.next() {
                match key.trim()  {
                    "MouseSpeedMultiplier" => if let Some(value) = tokens.next() {
                        if let Ok(value) = value.trim().parse() { 
                            println!("Setting mouse speed multiplier to {value}");
                            mouse_speed_multiplier = value;
                        } else {
                            panic!("In config file, MouseSpeedMultiplier is not a float.");
                        }
                    },
                    "MaxVolume" => if let Some(value) = tokens.next() {
                        if let Ok(value) = value.trim().parse() { 
                            println!("Setting max volume to {value}");
                            max_volume = value;
                        } else {
                            panic!("In config file, MaxVolume is not a float.");
                        }
                    },
                    "EraserRadius" => if let Some(value) = tokens.next() {
                        if let Ok(value) = value.trim().parse() { 
                            println!("Setting EraserRadius to {value}");
                            eraser_radius = value;
                        } else {
                            panic!("In config file, EraserRadius is not a float.");
                        }
                    },
                    _ => (),
                }
            }
        }
    } else {
        let base_config = "# How much louder will chalk sound with faster mouse movement:
MouseSpeedMultiplier=0.1
# What is the absolute maximum volume to output. 1.0 is \"normal\" but higher values can make it louder.
MaxVolume=1.0
# Eraser (right click) radius in pixels
EraserRadius=16.0
";
        match File::create(config_file_path) {
            Err(why) => panic!("couldn't create {}: {}", config_file_path, why),
            Ok(mut file) => match file.write_all(base_config.as_bytes()) {
                Err(why) => panic!("couldn't write to {}: {}", config_file_path, why),
                Ok(_) => println!("successfully wrote to {}", config_file_path),
            },
        }
    }
    app.insert_resource(FileSettings{mouse_speed_multiplier, max_volume, eraser_radius});
    app.insert_resource(CurrentColor(0))
        .insert_resource(UndoHistory(vec![Vec::new()]))
        .add_systems(Startup, setup)
        // .add_systems(PostStartup, set_color)
        .add_systems(Update, (on_mouse_move, stop_sound_if_mouse_stopped, set_color, on_chalk_interaction, undo, fullscreen, clear_all, eraser))
        ;
    app.run();
}
