use bevy::prelude::*;
use crate::common::types::{AppState, LevelEntity};
use crate::enemy::ai::Enemy;
use crate::player::Player;

#[derive(Component)]
pub struct MainMenuRoot;

#[derive(Component)]
pub struct TestLevel1Button;

#[derive(Component)]
pub struct LevelOutcomeBanner;

#[derive(Resource, Default)]
pub struct LevelEndManager {
    pub timer: Option<Timer>,
    pub outcome: Option<LevelOutcome>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LevelOutcome {
    Victory,
    Defeat,
}

const BUTTON_NORMAL: Color = Color::srgb(0.13, 0.18, 0.24);
const BUTTON_HOVER: Color = Color::srgb(0.22, 0.32, 0.44);
const BUTTON_PRESSED: Color = Color::srgb(0.16, 0.55, 0.85);

// Spawns clean tactical Main Menu UI on startup / menu state
pub fn setup_main_menu(mut commands: Commands) {
    commands
        .spawn((
            MainMenuRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(24.0),
                padding: UiRect::all(Val::Px(32.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.06, 0.08, 0.11, 0.95)),
        ))
        .with_children(|parent| {
            // Header / Title block
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|header| {
                    header.spawn((
                        Text::new("MIL-SIM"),
                        TextFont {
                            font_size: FontSize::Px(48.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.92, 0.96, 1.0)),
                    ));
                    header.spawn((
                        Text::new("TACTICAL SQUAD ENGAGEMENT SIMULATOR"),
                        TextFont {
                            font_size: FontSize::Px(15.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.25, 0.72, 0.92)),
                    ));
                });

            // Mission Selection Box
            parent
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(14.0),
                        padding: UiRect::axes(Val::Px(28.0), Val::Px(20.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.09, 0.12, 0.17, 0.8)),
                    BorderColor::all(Color::srgb(0.18, 0.26, 0.36)),
                ))
                .with_children(|section| {
                    section.spawn((
                        Text::new("AVAILABLE OPERATIONS"),
                        TextFont {
                            font_size: FontSize::Px(14.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.60, 0.68, 0.76)),
                    ));

                    // Button: test_level_1
                    section
                        .spawn((
                            Button,
                            TestLevel1Button,
                            Node {
                                width: Val::Px(360.0),
                                height: Val::Px(64.0),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                border: UiRect::all(Val::Px(1.5)),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(BUTTON_NORMAL),
                            BorderColor::all(Color::srgb(0.25, 0.38, 0.52)),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("▶  test_level_1"),
                                TextFont {
                                    font_size: FontSize::Px(22.0),
                                    ..default()
                                },
                                TextColor(Color::srgb(1.0, 1.0, 1.0)),
                            ));
                        });

                    section.spawn((
                        Text::new("Compound Infiltration • 3-Man Squad • 5 Hostile Guards"),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.48, 0.56, 0.64)),
                    ));
                });

            // Tactical Briefing & Controls Card
            parent
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(6.0),
                        padding: UiRect::axes(Val::Px(24.0), Val::Px(14.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.07, 0.09, 0.13, 0.7)),
                    BorderColor::all(Color::srgb(0.15, 0.20, 0.28)),
                ))
                .with_children(|controls| {
                    controls.spawn((
                        Text::new("CONTROLS & TACTICAL PROTOCOL"),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.25, 0.72, 0.92)),
                    ));
                    controls.spawn((
                        Text::new("• WASD: Squad Movement  |  Mouse: Aim & Vision Orientation"),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.75, 0.80, 0.85)),
                    ));
                    controls.spawn((
                        Text::new("• SPACE: Fire Weapon  |  C: Toggle Crouch  |  V: Toggle Prone"),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.75, 0.80, 0.85)),
                    ));
                    controls.spawn((
                        Text::new("• Crouch behind low walls for 100% cover protection"),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.75, 0.80, 0.85)),
                    ));
                    controls.spawn((
                        Text::new("• ESC: Abort mission and return to Main Menu"),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.55, 0.60, 0.65)),
                    ));
                });
        });
}

// Handles mouse interactions on the test_level_1 button
pub fn update_main_menu_interactions(
    mut next_state: ResMut<NextState<AppState>>,
    mut interaction_q: Query<
        (&Interaction, &mut BackgroundColor, &mut BorderColor),
        (Changed<Interaction>, With<TestLevel1Button>),
    >,
) {
    for (interaction, mut bg_color, mut border_color) in &mut interaction_q {
        match *interaction {
            Interaction::Pressed => {
                *bg_color = BUTTON_PRESSED.into();
                *border_color = BorderColor::all(Color::srgb(0.4, 0.8, 1.0));
                next_state.set(AppState::InGame);
            }
            Interaction::Hovered => {
                *bg_color = BUTTON_HOVER.into();
                *border_color = BorderColor::all(Color::srgb(0.35, 0.55, 0.75));
            }
            Interaction::None => {
                *bg_color = BUTTON_NORMAL.into();
                *border_color = BorderColor::all(Color::srgb(0.25, 0.38, 0.52));
            }
        }
    }
}

// Despawns Main Menu UI when transitioning to InGame
pub fn cleanup_main_menu(mut commands: Commands, menu_q: Query<Entity, With<MainMenuRoot>>) {
    for entity in &menu_q {
        commands.entity(entity).despawn();
    }
}

// Monitors level end conditions (player killed OR all enemies eliminated),
// displays outcome banner, and returns to the main menu after brief delay
pub fn check_level_end_conditions(
    mut commands: Commands,
    time: Res<Time>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut level_end: ResMut<LevelEndManager>,
    player_q: Query<Entity, With<Player>>,
    enemy_q: Query<Entity, With<Enemy>>,
    banner_q: Query<Entity, With<LevelOutcomeBanner>>,
) {
    // ESC key allows returning to menu at any time
    if keyboard_input.just_pressed(KeyCode::Escape) {
        level_end.timer = None;
        level_end.outcome = None;
        next_state.set(AppState::MainMenu);
        return;
    }

    // If countdown timer is currently active, tick and transition when finished
    if let Some(ref mut timer) = level_end.timer {
        timer.tick(time.delta());
        if timer.is_finished() {
            level_end.timer = None;
            level_end.outcome = None;
            next_state.set(AppState::MainMenu);
        }
        return;
    }

    let player_alive = !player_q.is_empty();
    let enemies_alive = !enemy_q.is_empty();

    // Trigger end condition if player was killed or all enemies neutralized
    if !player_alive {
        level_end.outcome = Some(LevelOutcome::Defeat);
        level_end.timer = Some(Timer::from_seconds(2.0, TimerMode::Once));

        if banner_q.is_empty() {
            spawn_outcome_banner(
                &mut commands,
                "MISSION FAILED — PLAYER KIA",
                "Returning to Main Menu...",
                Color::srgb(0.92, 0.15, 0.15),
            );
        }
    } else if !enemies_alive {
        level_end.outcome = Some(LevelOutcome::Victory);
        level_end.timer = Some(Timer::from_seconds(2.2, TimerMode::Once));

        if banner_q.is_empty() {
            spawn_outcome_banner(
                &mut commands,
                "MISSION ACCOMPLISHED — COMPOUND SECURED",
                "All hostile guards eliminated. Returning to Main Menu...",
                Color::srgb(0.18, 0.85, 0.35),
            );
        }
    }
}

fn spawn_outcome_banner(
    commands: &mut Commands,
    title: &str,
    subtitle: &str,
    accent_color: Color,
) {
    commands
        .spawn((
            LevelOutcomeBanner,
            LevelEntity,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(35.0),
                left: Val::Percent(15.0),
                right: Val::Percent(15.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(36.0), Val::Px(24.0)),
                row_gap: Val::Px(8.0),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.07, 0.10, 0.94)),
            BorderColor::all(accent_color),
        ))
        .with_children(|banner| {
            banner.spawn((
                Text::new(title),
                TextFont {
                    font_size: FontSize::Px(32.0),
                    ..default()
                },
                TextColor(accent_color),
            ));
            banner.spawn((
                Text::new(subtitle),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.88, 0.92)),
            ));
        });
}

// Despawns all level-specific entities when exiting InGame state
pub fn cleanup_level(
    mut commands: Commands,
    mut level_end: ResMut<LevelEndManager>,
    level_entities_q: Query<Entity, With<LevelEntity>>,
) {
    level_end.timer = None;
    level_end.outcome = None;

    for entity in &level_entities_q {
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_button_interaction_launches_level() {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.init_state::<AppState>();

        let button_entity = app
            .world_mut()
            .spawn((
                Button,
                TestLevel1Button,
                Interaction::Pressed,
                BackgroundColor(BUTTON_NORMAL),
                BorderColor::all(Color::srgb(0.2, 0.3, 0.4)),
            ))
            .id();

        app.add_systems(Update, update_main_menu_interactions);
        app.update(); // Processes interaction, sets next_state
        app.update(); // Applies StateTransition schedule

        let state = app.world().resource::<State<AppState>>();
        assert_eq!(*state.get(), AppState::InGame);

        let bg_color = app.world().get::<BackgroundColor>(button_entity).unwrap();
        assert_eq!(bg_color.0, BUTTON_PRESSED);
    }

    #[test]
    fn test_player_killed_triggers_defeat_and_menu_transition() {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.insert_state(AppState::InGame);
        app.init_resource::<Time>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<LevelEndManager>();

        // Spawn an enemy so enemies are alive, but NO player (player KIA)
        app.world_mut().spawn(Enemy {
            state: crate::enemy::ai::EnemyAiState::Search,
            current_waypoint_idx: 0,
            last_known_player_pos: None,
            investigate_timer: Timer::from_seconds(3.0, TimerMode::Once),
            shoot_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
            speed: 5.5,
            vision_range: 24.0,
            vision_half_angle: 45.0_f32.to_radians(),
            inner_radius: 2.0,
            stuck_timer: 0.0,
            last_pos: Vec2::ZERO,
        });

        app.add_systems(Update, check_level_end_conditions);

        // Frame 1: End condition triggers Defeat
        app.update();

        let level_end = app.world().resource::<LevelEndManager>();
        assert_eq!(level_end.outcome, Some(LevelOutcome::Defeat));
        assert!(level_end.timer.is_some());

        // Advance timer past 2.0 seconds
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(2100));

        // Frame 2: Timer expires and calls next_state.set(AppState::MainMenu)
        app.update();
        // Frame 3: StateTransition applies next_state -> AppState::MainMenu
        app.update();

        let state = app.world().resource::<State<AppState>>();
        assert_eq!(*state.get(), AppState::MainMenu);
    }

    #[test]
    fn test_all_enemies_killed_triggers_victory_and_menu_transition() {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.insert_state(AppState::InGame);
        app.init_resource::<Time>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<LevelEndManager>();

        // Spawn living player, but NO enemies
        app.world_mut().spawn((
            Player,
            Transform::from_xyz(0.0, 0.05, 48.0),
        ));

        app.add_systems(Update, check_level_end_conditions);

        // Frame 1: End condition triggers Victory
        app.update();

        let level_end = app.world().resource::<LevelEndManager>();
        assert_eq!(level_end.outcome, Some(LevelOutcome::Victory));
        assert!(level_end.timer.is_some());

        // Advance timer past 2.2 seconds
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(2300));

        // Frame 2: Timer expires and calls next_state.set(AppState::MainMenu)
        app.update();
        // Frame 3: StateTransition applies next_state -> AppState::MainMenu
        app.update();

        let state = app.world().resource::<State<AppState>>();
        assert_eq!(*state.get(), AppState::MainMenu);
    }

    #[test]
    fn test_cleanup_level_removes_all_level_entities() {
        let mut app = App::new();
        app.init_resource::<LevelEndManager>();

        let e1 = app.world_mut().spawn(LevelEntity).id();
        let e2 = app.world_mut().spawn(LevelEntity).id();
        let e3 = app.world_mut().spawn(LevelEntity).id();
        let regular = app.world_mut().spawn_empty().id();

        app.add_systems(Update, cleanup_level);
        app.update();

        assert!(app.world().get_entity(e1).is_err());
        assert!(app.world().get_entity(e2).is_err());
        assert!(app.world().get_entity(e3).is_err());
        assert!(app.world().get_entity(regular).is_ok());
    }
}
