//! Hey Verse — a native Bevy port of the Hey 3D "visit a friend's home" world
//! (the Android version is Godot 4.6; Godot can't be hosted inside the egui app,
//! so the Verse is its own native Rust binary, launched from the desktop app).
//!
//! v1 is a LOCAL SIM: your isometric home plot, a chibi-robot avatar, click-the-
//! ground to walk (point-and-go, the same control the Godot world uses), a follow
//! camera, and a HUD to recolor your avatar. Networking (real visitors over the
//! carrier via hey-verse-bridge → hey-mobile-runtime) is a later phase.

use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Hey Verse".into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.55, 0.73, 0.92))) // sky
        .init_resource::<Destination>()
        .add_systems(Startup, setup)
        .add_systems(Update, (click_to_move, move_player, camera_follow, color_button))
        .run();
}

// ── components / resources ────────────────────────────────────────────────────
#[derive(Component)]
struct Player;

#[derive(Component)]
struct MainCam;

#[derive(Component)]
struct ColorButton;

/// Where the avatar is walking to (point-and-go). None = idle.
#[derive(Resource, Default)]
struct Destination(Option<Vec3>);

/// The avatar's recolorable body palette + the live body material handle.
#[derive(Resource)]
struct AvatarLook {
    palette: Vec<Color>,
    idx: usize,
    body_mat: Handle<StandardMaterial>,
}

const FLOOR_Y: f32 = 0.3; // top of the home platform — the avatar's feet plane

// ── world build ───────────────────────────────────────────────────────────────
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Camera — a gentle isometric angle.
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(9.0, 11.0, 9.0)
                .looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
            ..default()
        },
        MainCam,
    ));

    // Lighting — soft sky ambient + a warm key light that casts shadows.
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.90, 0.93, 1.0),
        brightness: 350.0,
    });
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 9000.0,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(6.0, 12.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });

    // Grass ground.
    commands.spawn(PbrBundle {
        mesh: meshes.add(Plane3d::default().mesh().size(40.0, 40.0)),
        material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.42, 0.62, 0.34),
            perceptual_roughness: 0.95,
            ..default()
        }),
        ..default()
    });

    // Raised home platform.
    let floor = materials.add(Color::srgb(0.78, 0.70, 0.55));
    commands.spawn(PbrBundle {
        mesh: meshes.add(Cuboid::new(10.0, 0.3, 10.0)),
        material: floor,
        transform: Transform::from_xyz(0.0, 0.15, 0.0),
        ..default()
    });

    // Rug.
    commands.spawn(PbrBundle {
        mesh: meshes.add(Cuboid::new(3.6, 0.02, 2.6)),
        material: materials.add(Color::srgb(0.80, 0.30, 0.30)),
        transform: Transform::from_xyz(0.5, FLOOR_Y + 0.01, 1.2),
        ..default()
    });

    // Two low walls forming a corner.
    let wall = materials.add(Color::srgb(0.93, 0.91, 0.87));
    commands.spawn(PbrBundle {
        mesh: meshes.add(Cuboid::new(10.0, 2.4, 0.2)),
        material: wall.clone(),
        transform: Transform::from_xyz(0.0, 1.2, -5.0),
        ..default()
    });
    commands.spawn(PbrBundle {
        mesh: meshes.add(Cuboid::new(0.2, 2.4, 10.0)),
        material: wall,
        transform: Transform::from_xyz(-5.0, 1.2, 0.0),
        ..default()
    });

    // A couch + a round table (decor).
    commands.spawn(PbrBundle {
        mesh: meshes.add(Cuboid::new(3.0, 0.8, 1.0)),
        material: materials.add(Color::srgb(0.30, 0.45, 0.65)),
        transform: Transform::from_xyz(-3.4, FLOOR_Y + 0.4, -2.0),
        ..default()
    });
    commands.spawn(PbrBundle {
        mesh: meshes.add(Cylinder::new(0.7, 0.6)),
        material: materials.add(Color::srgb(0.55, 0.40, 0.25)),
        transform: Transform::from_xyz(2.6, FLOOR_Y + 0.3, 1.4),
        ..default()
    });

    // ── avatar (a chibi robot, built from primitives) ───────────────────────────
    let palette = vec![
        Color::srgb(0.84, 0.72, 0.29), // Hey gold
        Color::srgb(0.30, 0.62, 0.86), // sky
        Color::srgb(0.86, 0.40, 0.62), // rose
        Color::srgb(0.45, 0.78, 0.52), // mint
        Color::srgb(0.80, 0.50, 0.30), // amber
    ];
    let body_mat = materials.add(StandardMaterial {
        base_color: palette[0],
        perceptual_roughness: 0.5,
        metallic: 0.1,
        ..default()
    });
    let head_mat = materials.add(Color::srgb(0.95, 0.92, 0.85));
    let visor_mat = materials.add(Color::srgb(0.10, 0.12, 0.18));
    let eye_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.6, 0.95, 1.0),
        emissive: LinearRgba::rgb(0.4, 1.6, 2.2),
        ..default()
    });

    let body = meshes.add(Capsule3d::new(0.45, 0.7));
    let head = meshes.add(Cuboid::new(0.7, 0.6, 0.6));
    let visor = meshes.add(Cuboid::new(0.56, 0.18, 0.06));
    let eye = meshes.add(Sphere::new(0.07));

    commands
        .spawn((Player, SpatialBundle::from_transform(Transform::from_xyz(0.0, FLOOR_Y, 0.0))))
        .with_children(|p| {
            p.spawn(PbrBundle { mesh: body, material: body_mat.clone(), transform: Transform::from_xyz(0.0, 0.8, 0.0), ..default() });
            p.spawn(PbrBundle { mesh: head, material: head_mat, transform: Transform::from_xyz(0.0, 1.55, 0.0), ..default() });
            p.spawn(PbrBundle { mesh: visor, material: visor_mat, transform: Transform::from_xyz(0.0, 1.57, 0.28), ..default() });
            p.spawn(PbrBundle { mesh: eye.clone(), material: eye_mat.clone(), transform: Transform::from_xyz(-0.14, 1.59, 0.31), ..default() });
            p.spawn(PbrBundle { mesh: eye, material: eye_mat, transform: Transform::from_xyz(0.14, 1.59, 0.31), ..default() });
        });

    commands.insert_resource(AvatarLook { palette, idx: 0, body_mat });

    // ── HUD ──────────────────────────────────────────────────────────────────
    commands
        .spawn(NodeBundle {
            style: Style {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::all(Val::Px(18.0)),
                ..default()
            },
            ..default()
        })
        .with_children(|root| {
            // Top bar — wordmark + hint.
            root.spawn(NodeBundle {
                style: Style { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.0), ..default() },
                ..default()
            })
            .with_children(|c| {
                c.spawn(TextBundle::from_section(
                    "Hey Verse",
                    TextStyle { font_size: 28.0, color: Color::srgb(0.95, 0.84, 0.42), ..default() },
                ));
                c.spawn(TextBundle::from_section(
                    "Your home · click the ground to walk",
                    TextStyle { font_size: 14.0, color: Color::srgb(0.92, 0.94, 0.98), ..default() },
                ));
            });

            // Bottom — recolor button.
            root.spawn(NodeBundle {
                style: Style { column_gap: Val::Px(10.0), ..default() },
                ..default()
            })
            .with_children(|c| {
                c.spawn((
                    ButtonBundle {
                        style: Style {
                            padding: UiRect::axes(Val::Px(16.0), Val::Px(10.0)),
                            ..default()
                        },
                        background_color: Color::srgb(0.84, 0.72, 0.29).into(),
                        border_radius: BorderRadius::all(Val::Px(10.0)),
                        ..default()
                    },
                    ColorButton,
                ))
                .with_children(|b| {
                    b.spawn(TextBundle::from_section(
                        "Change avatar color",
                        TextStyle { font_size: 15.0, color: Color::srgb(0.05, 0.07, 0.12), ..default() },
                    ));
                });
            });
        });
}

// ── systems ───────────────────────────────────────────────────────────────────

/// Left-click the ground → set the walk destination (ray vs the y=FLOOR plane).
fn click_to_move(
    windows: Query<&Window>,
    cam_q: Query<(&Camera, &GlobalTransform), With<MainCam>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut dest: ResMut<Destination>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let Ok(window) = windows.get_single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok((camera, cam_tf)) = cam_q.get_single() else { return };
    let Some(ray) = camera.viewport_to_world(cam_tf, cursor) else { return };
    let dir = *ray.direction;
    if dir.y.abs() < 1e-5 {
        return;
    }
    let t = (FLOOR_Y - ray.origin.y) / dir.y;
    if t < 0.0 {
        return;
    }
    let hit = ray.origin + dir * t;
    // Keep the avatar on its plot (clamp to the platform footprint).
    let x = hit.x.clamp(-4.6, 4.6);
    let z = hit.z.clamp(-4.6, 4.6);
    dest.0 = Some(Vec3::new(x, FLOOR_Y, z));
}

/// Step the avatar toward its destination, facing the way it moves.
fn move_player(time: Res<Time>, mut dest: ResMut<Destination>, mut q: Query<&mut Transform, With<Player>>) {
    let Ok(mut tf) = q.get_single_mut() else { return };
    let Some(target) = dest.0 else { return };
    let mut to = target - tf.translation;
    to.y = 0.0;
    let dist = to.length();
    if dist < 0.05 {
        dest.0 = None;
        return;
    }
    let dir = to / dist;
    let step = (3.6 * time.delta_seconds()).min(dist);
    tf.translation += dir * step;
    tf.translation.y = FLOOR_Y;
    tf.rotation = Quat::from_rotation_y(dir.x.atan2(dir.z));
}

/// Smoothly trail the avatar with the isometric camera.
fn camera_follow(
    time: Res<Time>,
    player: Query<&Transform, (With<Player>, Without<MainCam>)>,
    mut cam: Query<&mut Transform, (With<MainCam>, Without<Player>)>,
) {
    let Ok(p) = player.get_single() else { return };
    let Ok(mut c) = cam.get_single_mut() else { return };
    let target = p.translation + Vec3::new(9.0, 11.0, 9.0);
    let k = (time.delta_seconds() * 3.0).min(1.0);
    c.translation = c.translation.lerp(target, k);
    c.look_at(p.translation + Vec3::new(0.0, 1.0, 0.0), Vec3::Y);
}

/// Cycle the avatar body color when the HUD button is pressed.
fn color_button(
    q: Query<&Interaction, (Changed<Interaction>, With<ColorButton>)>,
    mut look: ResMut<AvatarLook>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            look.idx = (look.idx + 1) % look.palette.len();
            let c = look.palette[look.idx];
            if let Some(m) = mats.get_mut(&look.body_mat) {
                m.base_color = c;
            }
        }
    }
}
