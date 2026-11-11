use bevy::prelude::*;

#[derive(Component)]
pub struct FpsText;

pub fn setup_fps_ui(mut commands: Commands) {
    commands.spawn((
        Text::new("FPS: --"),
        TextFont {
            font_size: FontSize::Px(18.0),
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.95, 0.2)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
        FpsText,
    ));
}

pub fn update_fps(
    time: Res<Time>,
    mut timer: Local<f32>,
    mut frames: Local<u32>,
    mut query: Query<&mut Text, With<FpsText>>,
) {
    *timer += time.delta_secs();
    *frames += 1;

    // Update readout roughly 4 times per second to prevent rapid visual jitter
    if *timer >= 0.25 {
        let fps = *frames as f32 / *timer;
        for mut text in &mut query {
            **text = format!("FPS: {:.0}", fps);
        }
        *timer = 0.0;
        *frames = 0;
    }
}
