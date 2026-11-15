use crate::common::types::{
    ApcOrder, CommandGroup, CommanderOrder, CurrentLevel, FogOfWarEnabled, HelicopterOrder,
    LevelEntity, OrderType, RTS_SOLDIER_WORLD_BOUND, SelectedOrderTool, Team,
};
use crate::friendly::SquadDoctrineState;
use crate::vehicle::{ApcVehicle, AttackHelicopter, VehicleOrder};
use bevy::prelude::*;
use std::f32::consts::FRAC_PI_2;

#[derive(Component)]
pub struct OrdersHudRoot;

#[derive(Component)]
pub struct InfantryGroupButton;

#[derive(Component)]
pub struct ApcGroupButton;

#[derive(Component)]
pub struct HelicopterGroupButton;

#[derive(Component)]
pub struct TakePositionOrderButton;

#[derive(Component)]
pub struct ClearOrderButton;

#[derive(Component)]
pub struct FogToggleButton;

#[derive(Component)]
pub struct FogToggleButtonText;

#[derive(Component)]
pub struct ApcCrewToggleButton;

#[derive(Component)]
pub struct ApcCrewToggleButtonText;

#[derive(Component)]
pub struct OrderStatusText;

#[derive(Component)]
pub struct SquadDoctrineText;

#[derive(Component)]
pub struct OrderObjectiveMarker;

#[derive(Component)]
pub struct ApcOrderObjectiveMarker;

#[derive(Component)]
pub struct HelicopterOrderObjectiveMarker;

pub const ORDER_BTN_NORMAL: Color = Color::srgb(0.13, 0.18, 0.24);
pub const ORDER_BTN_HOVER: Color = Color::srgb(0.22, 0.32, 0.44);
pub const ORDER_BTN_SELECTED: Color = Color::srgb(0.14, 0.56, 0.42);
pub const GROUP_BTN_SELECTED: Color = Color::srgb(0.18, 0.46, 0.72);
pub const CLEAR_BTN_NORMAL: Color = Color::srgb(0.24, 0.14, 0.16);
pub const CLEAR_BTN_HOVER: Color = Color::srgb(0.36, 0.20, 0.22);

/// Spawns the RTS Commander Orders toolbar and 3D ground objective markers for `test_level_4`.
pub fn setup_orders_ui(
    mut commands: Commands,
    current_level: Option<Res<CurrentLevel>>,
    mut selected_tool: Option<ResMut<SelectedOrderTool>>,
    mut selected_group: Option<ResMut<CommandGroup>>,
    mut commander_order: Option<ResMut<CommanderOrder>>,
    mut apc_order: Option<ResMut<ApcOrder>>,
    mut helicopter_order: Option<ResMut<HelicopterOrder>>,
    mut squad_doctrine: Option<ResMut<SquadDoctrineState>>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    if let Some(ref mut tool) = selected_tool {
        tool.0 = None;
    }
    if let Some(ref mut group) = selected_group {
        **group = CommandGroup::InfantrySquad;
    }
    if let Some(ref mut order) = commander_order {
        **order = CommanderOrder::Hold;
    }
    if let Some(ref mut v_order) = apc_order {
        v_order.0 = CommanderOrder::Hold;
    }
    if let Some(ref mut h_order) = helicopter_order {
        h_order.0 = CommanderOrder::Hold;
    }
    if let Some(ref mut doctrine) = squad_doctrine {
        **doctrine = SquadDoctrineState::default();
    }

    if current_level.as_deref().copied().unwrap_or_default() != CurrentLevel::Level4 {
        return;
    }

    // Spawn 3D ground markers for active objective positions (Infantry Squad in emerald, APC in amber, Helicopter in cyan)
    if let (Some(meshes), Some(materials)) = (meshes.as_mut(), materials.as_mut()) {
        let outer_mesh = meshes.add(Circle::new(3.2));
        let inner_mesh = meshes.add(Circle::new(0.8));
        let outer_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.18, 0.90, 0.62, 0.30),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });
        let inner_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.30, 1.0, 0.72, 0.85),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });

        let marker = commands
            .spawn((
                LevelEntity,
                OrderObjectiveMarker,
                Mesh3d(outer_mesh.clone()),
                MeshMaterial3d(outer_mat),
                Transform::from_xyz(0.0, 0.02, 0.0)
                    .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                Visibility::Hidden,
            ))
            .id();

        commands.spawn((
            Mesh3d(inner_mesh.clone()),
            MeshMaterial3d(inner_mat),
            Transform::from_xyz(0.0, 0.0, 0.002),
            ChildOf(marker),
        ));

        let apc_outer_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.95, 0.68, 0.18, 0.32),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });
        let apc_inner_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.82, 0.28, 0.88),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });

        let apc_marker = commands
            .spawn((
                LevelEntity,
                ApcOrderObjectiveMarker,
                Mesh3d(outer_mesh.clone()),
                MeshMaterial3d(apc_outer_mat),
                Transform::from_xyz(0.0, 0.022, 0.0)
                    .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                Visibility::Hidden,
            ))
            .id();

        commands.spawn((
            Mesh3d(inner_mesh.clone()),
            MeshMaterial3d(apc_inner_mat),
            Transform::from_xyz(0.0, 0.0, 0.002),
            ChildOf(apc_marker),
        ));

        let heli_outer_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.22, 0.78, 0.98, 0.32),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });
        let heli_inner_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.45, 0.90, 1.0, 0.88),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });

        let heli_marker = commands
            .spawn((
                LevelEntity,
                HelicopterOrderObjectiveMarker,
                Mesh3d(outer_mesh),
                MeshMaterial3d(heli_outer_mat),
                Transform::from_xyz(0.0, 0.024, 0.0)
                    .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                Visibility::Hidden,
            ))
            .id();

        commands.spawn((
            Mesh3d(inner_mesh),
            MeshMaterial3d(heli_inner_mat),
            Transform::from_xyz(0.0, 0.0, 0.002),
            ChildOf(heli_marker),
        ));
    }

    commands
        .spawn((
            OrdersHudRoot,
            LevelEntity,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(20.0),
                left: Val::Px(24.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(16.0), Val::Px(12.0)),
                row_gap: Val::Px(8.0),
                border: UiRect::all(Val::Px(1.5)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                min_width: Val::Px(360.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.07, 0.10, 0.14, 0.90)),
            BorderColor::all(Color::srgb(0.22, 0.34, 0.48)),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("COMMANDER ORDERS  •  SELECT UNIT GROUP ([Q] / [E] / [R] / [TAB])"),
                TextFont {
                    font_size: FontSize::Px(11.0),
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.68, 0.82)),
            ));

            // Unit group selection row (Group 1: Infantry Squad, Group 2: APC Vehicle, Group 3: Attack Helicopter)
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(10.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|group_row| {
                group_row
                    .spawn((
                        Button,
                        InfantryGroupButton,
                        Node {
                            padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: UiRect::all(Val::Px(1.5)),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(GROUP_BTN_SELECTED),
                        BorderColor::all(Color::srgb(0.32, 0.64, 0.90)),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("[Q] GROUP 1: INFANTRY SQUAD"),
                            TextFont {
                                font_size: FontSize::Px(12.5),
                                ..default()
                            },
                            TextColor(Color::srgb(0.95, 0.98, 1.0)),
                        ));
                    });

                group_row
                    .spawn((
                        Button,
                        ApcGroupButton,
                        Node {
                            padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: UiRect::all(Val::Px(1.5)),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(ORDER_BTN_NORMAL),
                        BorderColor::all(Color::srgb(0.65, 0.52, 0.24)),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("[E] GROUP 2: APC VEHICLE"),
                            TextFont {
                                font_size: FontSize::Px(12.5),
                                ..default()
                            },
                            TextColor(Color::srgb(0.98, 0.92, 0.78)),
                        ));
                    });

                group_row
                    .spawn((
                        Button,
                        HelicopterGroupButton,
                        Node {
                            padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: UiRect::all(Val::Px(1.5)),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(ORDER_BTN_NORMAL),
                        BorderColor::all(Color::srgb(0.28, 0.62, 0.76)),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("[R] GROUP 3: HELICOPTER"),
                            TextFont {
                                font_size: FontSize::Px(12.5),
                                ..default()
                            },
                            TextColor(Color::srgb(0.84, 0.96, 1.0)),
                        ));
                    });
            });

            // Order type selection buttons row
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(10.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Button,
                    TakePositionOrderButton,
                    Node {
                        padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.5)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(ORDER_BTN_NORMAL),
                    BorderColor::all(Color::srgb(0.24, 0.48, 0.62)),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("[1] TAKE POSITION"),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.95, 0.98, 1.0)),
                    ));
                });

                row.spawn((
                    Button,
                    ClearOrderButton,
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.5)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(CLEAR_BTN_NORMAL),
                    BorderColor::all(Color::srgb(0.45, 0.25, 0.28)),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("[X] HOLD / CLEAR"),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.92, 0.85, 0.85)),
                    ));
                });

                row.spawn((
                    Button,
                    FogToggleButton,
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.5)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(ORDER_BTN_SELECTED),
                    BorderColor::all(Color::srgb(0.24, 0.56, 0.48)),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        FogToggleButtonText,
                        Text::new("[F] FOG OF WAR: ON"),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.95, 0.98, 1.0)),
                    ));
                });

                row.spawn((
                    Button,
                    ApcCrewToggleButton,
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.5)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(ORDER_BTN_SELECTED),
                    BorderColor::all(Color::srgb(0.24, 0.56, 0.48)),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        ApcCrewToggleButtonText,
                        Text::new("[G] APC CREW: MOUNTED"),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.95, 0.98, 1.0)),
                    ));
                });
            });

            root.spawn((
                OrderStatusText,
                Text::new("ACTIVE ORDER: HOLD  •  Select an order type ([1])"),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(Color::srgb(0.75, 0.84, 0.92)),
            ));

            root.spawn((
                SquadDoctrineText,
                Text::new("SQUAD 1: NORMAL (Following Commander Intent)"),
                TextFont {
                    font_size: FontSize::Px(11.5),
                    ..default()
                },
                TextColor(Color::srgb(0.95, 0.78, 0.35)),
            ));
        });
}

/// Handles unit group selection (`CommandGroup::InfantrySquad`, `CommandGroup::Apc`, `CommandGroup::Helicopter`),
/// order type selection (via UI buttons or hotkeys), debug fog toggle, and map click target assignment.
pub fn update_orders_ui(
    (current_level, keyboard_input, mouse_buttons, squad_doctrine, mut fog_enabled): (
        Option<Res<CurrentLevel>>,
        Option<Res<ButtonInput<KeyCode>>>,
        Option<Res<ButtonInput<MouseButton>>>,
        Option<Res<SquadDoctrineState>>,
        Option<ResMut<FogOfWarEnabled>>,
    ),
    (selected_tool, mut selected_group, commander_order, mut apc_order, mut helicopter_order): (
        Option<ResMut<SelectedOrderTool>>,
        Option<ResMut<CommandGroup>>,
        Option<ResMut<CommanderOrder>>,
        Option<ResMut<ApcOrder>>,
        Option<ResMut<HelicopterOrder>>,
    ),
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    (mut inf_group_btn_q, mut apc_group_btn_q, mut heli_group_btn_q): (
        Query<
            (&Interaction, &mut BackgroundColor),
            (
                With<InfantryGroupButton>,
                Without<ApcGroupButton>,
                Without<HelicopterGroupButton>,
                Without<TakePositionOrderButton>,
                Without<ClearOrderButton>,
                Without<FogToggleButton>,
                Without<ApcCrewToggleButton>,
            ),
        >,
        Query<
            (&Interaction, &mut BackgroundColor),
            (
                With<ApcGroupButton>,
                Without<InfantryGroupButton>,
                Without<HelicopterGroupButton>,
                Without<TakePositionOrderButton>,
                Without<ClearOrderButton>,
                Without<FogToggleButton>,
                Without<ApcCrewToggleButton>,
            ),
        >,
        Query<
            (&Interaction, &mut BackgroundColor),
            (
                With<HelicopterGroupButton>,
                Without<InfantryGroupButton>,
                Without<ApcGroupButton>,
                Without<TakePositionOrderButton>,
                Without<ClearOrderButton>,
                Without<FogToggleButton>,
                Without<ApcCrewToggleButton>,
            ),
        >,
    ),
    (mut take_pos_btn_q, mut clear_btn_q, mut fog_btn_q, mut apc_btn_q): (
        Query<
            (&Interaction, &mut BackgroundColor),
            (
                With<TakePositionOrderButton>,
                Without<InfantryGroupButton>,
                Without<ApcGroupButton>,
                Without<HelicopterGroupButton>,
                Without<ClearOrderButton>,
                Without<FogToggleButton>,
                Without<ApcCrewToggleButton>,
            ),
        >,
        Query<
            (&Interaction, &mut BackgroundColor),
            (
                With<ClearOrderButton>,
                Without<InfantryGroupButton>,
                Without<ApcGroupButton>,
                Without<HelicopterGroupButton>,
                Without<TakePositionOrderButton>,
                Without<FogToggleButton>,
                Without<ApcCrewToggleButton>,
            ),
        >,
        Query<
            (&Interaction, &mut BackgroundColor),
            (
                With<FogToggleButton>,
                Without<InfantryGroupButton>,
                Without<ApcGroupButton>,
                Without<HelicopterGroupButton>,
                Without<TakePositionOrderButton>,
                Without<ClearOrderButton>,
                Without<ApcCrewToggleButton>,
            ),
        >,
        Query<
            (&Interaction, &mut BackgroundColor),
            (
                With<ApcCrewToggleButton>,
                Without<InfantryGroupButton>,
                Without<ApcGroupButton>,
                Without<HelicopterGroupButton>,
                Without<TakePositionOrderButton>,
                Without<ClearOrderButton>,
                Without<FogToggleButton>,
            ),
        >,
    ),
    (mut fog_text_q, mut apc_text_q, mut status_text_q, mut doctrine_text_q): (
        Query<
            &mut Text,
            (
                With<FogToggleButtonText>,
                Without<OrderStatusText>,
                Without<SquadDoctrineText>,
                Without<ApcCrewToggleButtonText>,
            ),
        >,
        Query<
            &mut Text,
            (
                With<ApcCrewToggleButtonText>,
                Without<FogToggleButtonText>,
                Without<OrderStatusText>,
                Without<SquadDoctrineText>,
            ),
        >,
        Query<
            (&mut Text, &mut TextColor),
            (
                With<OrderStatusText>,
                Without<FogToggleButtonText>,
                Without<SquadDoctrineText>,
                Without<ApcCrewToggleButtonText>,
            ),
        >,
        Query<
            &mut Text,
            (
                With<SquadDoctrineText>,
                Without<OrderStatusText>,
                Without<FogToggleButtonText>,
                Without<ApcCrewToggleButtonText>,
            ),
        >,
    ),
    (mut apc_q, mut heli_q): (
        Query<
            (&Transform, &Team, &mut ApcVehicle),
            (
                Without<AttackHelicopter>,
                Without<OrderObjectiveMarker>,
                Without<ApcOrderObjectiveMarker>,
                Without<HelicopterOrderObjectiveMarker>,
                Without<Camera3d>,
            ),
        >,
        Query<
            (&Transform, &Team, &mut AttackHelicopter),
            (
                Without<ApcVehicle>,
                Without<OrderObjectiveMarker>,
                Without<ApcOrderObjectiveMarker>,
                Without<HelicopterOrderObjectiveMarker>,
                Without<Camera3d>,
            ),
        >,
    ),
    (mut marker_q, mut apc_marker_q, mut heli_marker_q): (
        Query<
            (&mut Transform, &mut Visibility),
            (
                With<OrderObjectiveMarker>,
                Without<ApcOrderObjectiveMarker>,
                Without<HelicopterOrderObjectiveMarker>,
                Without<Camera3d>,
            ),
        >,
        Query<
            (&mut Transform, &mut Visibility),
            (
                With<ApcOrderObjectiveMarker>,
                Without<OrderObjectiveMarker>,
                Without<HelicopterOrderObjectiveMarker>,
                Without<Camera3d>,
            ),
        >,
        Query<
            (&mut Transform, &mut Visibility),
            (
                With<HelicopterOrderObjectiveMarker>,
                Without<OrderObjectiveMarker>,
                Without<ApcOrderObjectiveMarker>,
                Without<Camera3d>,
            ),
        >,
    ),
) {
    if current_level.as_deref().copied().unwrap_or_default() != CurrentLevel::Level4 {
        return;
    }

    let (Some(mut selected_tool), Some(mut commander_order)) = (selected_tool, commander_order)
    else {
        return;
    };

    let mut ui_captured_click = false;

    let button_just_clicked = mouse_buttons
        .as_ref()
        .map_or(true, |mb| mb.just_pressed(MouseButton::Left));

    let mut active_group = selected_group
        .as_deref()
        .copied()
        .unwrap_or(CommandGroup::InfantrySquad);

    // 0. Unit Group Selection buttons (Infantry Squad, APC Vehicle, Attack Helicopter)
    for (interaction, _) in &mut inf_group_btn_q {
        match *interaction {
            Interaction::Pressed => {
                ui_captured_click = true;
                if button_just_clicked {
                    active_group = CommandGroup::InfantrySquad;
                }
            }
            Interaction::Hovered => {
                ui_captured_click = true;
            }
            Interaction::None => {}
        }
    }

    for (interaction, _) in &mut apc_group_btn_q {
        match *interaction {
            Interaction::Pressed => {
                ui_captured_click = true;
                if button_just_clicked {
                    active_group = CommandGroup::Apc;
                }
            }
            Interaction::Hovered => {
                ui_captured_click = true;
            }
            Interaction::None => {}
        }
    }

    for (interaction, _) in &mut heli_group_btn_q {
        match *interaction {
            Interaction::Pressed => {
                ui_captured_click = true;
                if button_just_clicked {
                    active_group = CommandGroup::Helicopter;
                }
            }
            Interaction::Hovered => {
                ui_captured_click = true;
            }
            Interaction::None => {}
        }
    }

    // 1. UI button interactions for selecting/clearing order types and toggling Fog of War
    for (interaction, mut bg) in &mut take_pos_btn_q {
        match *interaction {
            Interaction::Pressed => {
                ui_captured_click = true;
                if button_just_clicked {
                    selected_tool.0 = if selected_tool.0 == Some(OrderType::TakePosition) {
                        None
                    } else {
                        Some(OrderType::TakePosition)
                    };
                }
            }
            Interaction::Hovered => {
                ui_captured_click = true;
                *bg = BackgroundColor(if selected_tool.0 == Some(OrderType::TakePosition) {
                    ORDER_BTN_SELECTED
                } else {
                    ORDER_BTN_HOVER
                });
            }
            Interaction::None => {
                *bg = BackgroundColor(if selected_tool.0 == Some(OrderType::TakePosition) {
                    ORDER_BTN_SELECTED
                } else {
                    ORDER_BTN_NORMAL
                });
            }
        }
    }

    let mut clear_active_group_order = false;
    for (interaction, mut bg) in &mut clear_btn_q {
        match *interaction {
            Interaction::Pressed => {
                ui_captured_click = true;
                selected_tool.0 = None;
                clear_active_group_order = true;
                *bg = BackgroundColor(CLEAR_BTN_HOVER);
            }
            Interaction::Hovered => {
                ui_captured_click = true;
                *bg = BackgroundColor(CLEAR_BTN_HOVER);
            }
            Interaction::None => {
                *bg = BackgroundColor(CLEAR_BTN_NORMAL);
            }
        }
    }

    let mut toggle_fog = false;
    for (interaction, _) in &mut fog_btn_q {
        match *interaction {
            Interaction::Pressed => {
                ui_captured_click = true;
                if button_just_clicked {
                    toggle_fog = true;
                }
            }
            Interaction::Hovered => {
                ui_captured_click = true;
            }
            Interaction::None => {}
        }
    }

    let mut toggle_apc_crew = false;
    for (interaction, _) in &mut apc_btn_q {
        match *interaction {
            Interaction::Pressed => {
                ui_captured_click = true;
                if button_just_clicked {
                    toggle_apc_crew = true;
                }
            }
            Interaction::Hovered => {
                ui_captured_click = true;
            }
            Interaction::None => {}
        }
    }

    // 2. Keyboard shortcuts for group selection ([Q]/[E]/[R]/[Tab]), order types ([1]/[X]), or toggles ([F]/[G])
    if let Some(ref keys) = keyboard_input {
        if keys.just_pressed(KeyCode::KeyQ) {
            active_group = CommandGroup::InfantrySquad;
        }
        if keys.just_pressed(KeyCode::KeyE) {
            active_group = CommandGroup::Apc;
        }
        if keys.just_pressed(KeyCode::KeyR) {
            active_group = CommandGroup::Helicopter;
        }
        if keys.just_pressed(KeyCode::Tab) {
            active_group = match active_group {
                CommandGroup::InfantrySquad => CommandGroup::Apc,
                CommandGroup::Apc => CommandGroup::Helicopter,
                CommandGroup::Helicopter => CommandGroup::InfantrySquad,
            };
        }
        if keys.just_pressed(KeyCode::Digit1) {
            selected_tool.0 = if selected_tool.0 == Some(OrderType::TakePosition) {
                None
            } else {
                Some(OrderType::TakePosition)
            };
        }
        if keys.just_pressed(KeyCode::KeyF) {
            toggle_fog = true;
        }
        if keys.just_pressed(KeyCode::KeyG) {
            toggle_apc_crew = true;
        }
        if keys.just_pressed(KeyCode::Escape) {
            selected_tool.0 = None;
        }
        if keys.just_pressed(KeyCode::KeyX) || keys.just_pressed(KeyCode::Backspace) {
            selected_tool.0 = None;
            clear_active_group_order = true;
        }
    }

    if clear_active_group_order {
        match active_group {
            CommandGroup::InfantrySquad => {
                *commander_order = CommanderOrder::Hold;
            }
            CommandGroup::Apc => {
                if let Some(ref mut v_order) = apc_order {
                    v_order.0 = CommanderOrder::Hold;
                }
                for (_, team, mut apc) in &mut apc_q {
                    if *team == Team::Player && matches!(apc.order, VehicleOrder::MoveTo(_)) {
                        apc.order = VehicleOrder::Hold;
                    }
                }
            }
            CommandGroup::Helicopter => {
                if let Some(ref mut h_order) = helicopter_order {
                    h_order.0 = CommanderOrder::Hold;
                }
                for (_, team, mut heli) in &mut heli_q {
                    if *team == Team::Player && matches!(heli.order, VehicleOrder::MoveTo(_)) {
                        heli.order = VehicleOrder::Hold;
                    }
                }
            }
        }
    }

    if toggle_apc_crew {
        for (_, team, mut apc) in &mut apc_q {
            if *team == Team::Player {
                if apc.has_driver() || apc.has_gunner() {
                    apc.order = VehicleOrder::DismountCrew;
                } else {
                    apc.order = VehicleOrder::MountCrew;
                }
            }
        }
    }

    if toggle_fog {
        if let Some(ref mut fog) = fog_enabled {
            fog.0 = !fog.0;
        }
    }

    // 3. Map Left-Click handling:
    // - If an OrderType is armed, assign the clicked position to the currently selected group (`active_group`).
    // - If no OrderType is armed, clicking directly on the APC or Helicopter selects its group.
    let left_clicked = mouse_buttons
        .as_ref()
        .is_some_and(|mb| mb.just_pressed(MouseButton::Left));
    if left_clicked && !ui_captured_click {
        if let (Ok(window), Ok((camera, cam_global_tf))) = (windows.single(), camera_q.single()) {
            if let Some(cursor_pos) = window.cursor_position() {
                if let Ok(ray) = camera.viewport_to_world(cam_global_tf, cursor_pos) {
                    let world_xz = if ray.direction.y.abs() > 1e-4 {
                        let t = -ray.origin.y / ray.direction.y;
                        let hit = ray.origin + *ray.direction * t;
                        Vec2::new(hit.x, hit.z)
                    } else {
                        Vec2::new(ray.origin.x, ray.origin.z)
                    };
                    let clamped_target = Vec2::new(
                        world_xz
                            .x
                            .clamp(-RTS_SOLDIER_WORLD_BOUND, RTS_SOLDIER_WORLD_BOUND),
                        world_xz
                            .y
                            .clamp(-RTS_SOLDIER_WORLD_BOUND, RTS_SOLDIER_WORLD_BOUND),
                    );
                    if let Some(active_tool) = selected_tool.0 {
                        match active_tool {
                            OrderType::TakePosition => match active_group {
                                CommandGroup::InfantrySquad => {
                                    *commander_order =
                                        CommanderOrder::TakePosition(clamped_target);
                                }
                                CommandGroup::Apc => {
                                    if let Some(ref mut v_order) = apc_order {
                                        v_order.0 = CommanderOrder::TakePosition(clamped_target);
                                    }
                                    for (_, team, mut apc) in &mut apc_q {
                                        if *team == Team::Player
                                            && !matches!(
                                                apc.order,
                                                VehicleOrder::DismountCrew
                                                    | VehicleOrder::MountCrew
                                            )
                                        {
                                            apc.order = VehicleOrder::MoveTo(clamped_target);
                                        }
                                    }
                                }
                                CommandGroup::Helicopter => {
                                    if let Some(ref mut h_order) = helicopter_order {
                                        h_order.0 = CommanderOrder::TakePosition(clamped_target);
                                    }
                                    for (_, team, mut heli) in &mut heli_q {
                                        if *team == Team::Player {
                                            heli.order = VehicleOrder::MoveTo(clamped_target);
                                        }
                                    }
                                }
                            },
                        }
                        selected_tool.0 = None;
                    } else {
                        // Direct click on APC or Helicopter selects its group
                        let clicked_heli = heli_q.iter().any(|(h_tf, team, _)| {
                            *team == Team::Player
                                && (Vec2::new(h_tf.translation.x, h_tf.translation.z) - world_xz)
                                    .length()
                                    <= 6.5
                        });
                        let clicked_apc = apc_q.iter().any(|(v_tf, team, _)| {
                            *team == Team::Player
                                && (Vec2::new(v_tf.translation.x, v_tf.translation.z) - world_xz)
                                    .length()
                                    <= 5.5
                        });
                        if clicked_heli {
                            active_group = CommandGroup::Helicopter;
                        } else if clicked_apc {
                            active_group = CommandGroup::Apc;
                        }
                    }
                }
            }
        }
    }

    if let Some(ref mut group_res) = selected_group {
        **group_res = active_group;
    }

    // Sync group selection button backgrounds
    for (interaction, mut bg) in &mut inf_group_btn_q {
        *bg = BackgroundColor(if active_group == CommandGroup::InfantrySquad {
            GROUP_BTN_SELECTED
        } else if *interaction == Interaction::Hovered {
            ORDER_BTN_HOVER
        } else {
            ORDER_BTN_NORMAL
        });
    }
    for (interaction, mut bg) in &mut apc_group_btn_q {
        *bg = BackgroundColor(if active_group == CommandGroup::Apc {
            GROUP_BTN_SELECTED
        } else if *interaction == Interaction::Hovered {
            ORDER_BTN_HOVER
        } else {
            ORDER_BTN_NORMAL
        });
    }
    for (interaction, mut bg) in &mut heli_group_btn_q {
        *bg = BackgroundColor(if active_group == CommandGroup::Helicopter {
            GROUP_BTN_SELECTED
        } else if *interaction == Interaction::Hovered {
            ORDER_BTN_HOVER
        } else {
            ORDER_BTN_NORMAL
        });
    }

    let is_fog_on = fog_enabled.as_deref().map_or(true, |f| f.0);
    for (interaction, mut bg) in &mut fog_btn_q {
        *bg = BackgroundColor(if is_fog_on {
            ORDER_BTN_SELECTED
        } else if *interaction == Interaction::Hovered {
            ORDER_BTN_HOVER
        } else {
            ORDER_BTN_NORMAL
        });
    }
    for mut text in &mut fog_text_q {
        **text = if is_fog_on {
            "[F] FOG OF WAR: ON".to_string()
        } else {
            "[F] FOG OF WAR: OFF".to_string()
        };
    }

    let is_apc_crewed = apc_q
        .iter()
        .filter(|(_, t, _)| **t == Team::Player)
        .any(|(_, _, apc)| {
            apc.has_driver() || apc.has_gunner() || apc.order == VehicleOrder::MountCrew
        });
    for (interaction, mut bg) in &mut apc_btn_q {
        *bg = BackgroundColor(if is_apc_crewed {
            ORDER_BTN_SELECTED
        } else if *interaction == Interaction::Hovered {
            ORDER_BTN_HOVER
        } else {
            ORDER_BTN_NORMAL
        });
    }
    for mut text in &mut apc_text_q {
        **text = if is_apc_crewed {
            "[G] APC CREW: MOUNTED".to_string()
        } else {
            "[G] APC CREW: DISMOUNTED".to_string()
        };
    }

    // Sync TakePosition button background after keyboard toggle
    for (interaction, mut bg) in &mut take_pos_btn_q {
        if *interaction != Interaction::Hovered {
            *bg = BackgroundColor(if selected_tool.0 == Some(OrderType::TakePosition) {
                ORDER_BTN_SELECTED
            } else {
                ORDER_BTN_NORMAL
            });
        }
    }

    // 4. Update HUD status readout and 3D objective ground markers for all groups
    let current_apc_order = apc_order
        .as_deref()
        .map_or(CommanderOrder::Hold, |o| o.0);
    let current_heli_order = helicopter_order
        .as_deref()
        .map_or(CommanderOrder::Hold, |o| o.0);
    let format_order = |order: CommanderOrder| match order {
        CommanderOrder::Hold => "HOLD".to_string(),
        CommanderOrder::TakePosition(p) => format!("TAKE POSITION ({:.0}, {:.0})", p.x, p.y),
    };
    let group_label = match active_group {
        CommandGroup::InfantrySquad => "GROUP 1: INFANTRY SQUAD",
        CommandGroup::Apc => "GROUP 2: APC VEHICLE",
        CommandGroup::Helicopter => "GROUP 3: HELICOPTER",
    };
    let active_group_order = match active_group {
        CommandGroup::InfantrySquad => *commander_order,
        CommandGroup::Apc => current_apc_order,
        CommandGroup::Helicopter => current_heli_order,
    };

    for (mut text, mut color) in &mut status_text_q {
        if let Some(OrderType::TakePosition) = selected_tool.0 {
            **text = format!(
                "ORDER ARMED [{}]: TAKE POSITION  •  Left-click map to assign  ([ESC] Cancel)",
                group_label
            );
            *color = TextColor(Color::srgb(0.32, 0.95, 0.68));
        } else {
            **text = format!(
                "SELECTED [{}]  •  SQUAD: {}  |  APC: {}  |  HELI: {}  •  [1] Order  [X] Hold",
                group_label,
                format_order(*commander_order),
                format_order(current_apc_order),
                format_order(current_heli_order),
            );
            *color = match active_group_order {
                CommanderOrder::Hold => TextColor(Color::srgb(0.75, 0.84, 0.92)),
                CommanderOrder::TakePosition(_) => TextColor(Color::srgb(0.35, 0.88, 1.0)),
            };
        }
    }

    for (mut tf, mut vis) in &mut marker_q {
        match *commander_order {
            CommanderOrder::Hold => {
                *vis = Visibility::Hidden;
            }
            CommanderOrder::TakePosition(pos) => {
                tf.translation.x = pos.x;
                tf.translation.z = pos.y;
                *vis = Visibility::Visible;
            }
        }
    }

    for (mut tf, mut vis) in &mut apc_marker_q {
        match current_apc_order {
            CommanderOrder::Hold => {
                *vis = Visibility::Hidden;
            }
            CommanderOrder::TakePosition(pos) => {
                tf.translation.x = pos.x;
                tf.translation.z = pos.y;
                *vis = Visibility::Visible;
            }
        }
    }

    for (mut tf, mut vis) in &mut heli_marker_q {
        match current_heli_order {
            CommanderOrder::Hold => {
                *vis = Visibility::Hidden;
            }
            CommanderOrder::TakePosition(pos) => {
                tf.translation.x = pos.x;
                tf.translation.z = pos.y;
                *vis = Visibility::Visible;
            }
        }
    }

    let summary = squad_doctrine.as_deref().map_or_else(
        || "SQUAD 1: NORMAL (Following Commander Intent)".to_string(),
        |d| d.summary_for_squad(0),
    );
    for mut text in &mut doctrine_text_q {
        **text = summary.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_type_selection_and_clear_via_buttons_and_hotkeys() {
        let mut app = App::new();
        app.insert_resource(CurrentLevel::Level4);
        app.init_resource::<SelectedOrderTool>();
        app.init_resource::<CommandGroup>();
        app.init_resource::<CommanderOrder>();
        app.init_resource::<ApcOrder>();
        app.init_resource::<HelicopterOrder>();
        app.init_resource::<FogOfWarEnabled>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<ButtonInput<MouseButton>>();

        app.add_systems(Startup, setup_orders_ui);
        app.add_systems(Update, update_orders_ui);
        app.update();

        // Initially no tool is armed, InfantrySquad group is selected, orders are Hold, and Fog of War is enabled
        assert_eq!(app.world().resource::<SelectedOrderTool>().0, None);
        assert_eq!(
            *app.world().resource::<CommandGroup>(),
            CommandGroup::InfantrySquad
        );
        assert_eq!(
            *app.world().resource::<CommanderOrder>(),
            CommanderOrder::Hold
        );
        assert_eq!(
            app.world().resource::<ApcOrder>().0,
            CommanderOrder::Hold
        );
        assert_eq!(
            app.world().resource::<HelicopterOrder>().0,
            CommanderOrder::Hold
        );
        assert!(app.world().resource::<FogOfWarEnabled>().0);

        // Press [F] to toggle Fog of War OFF for debugging
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyF);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyF);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        assert!(
            !app.world().resource::<FogOfWarEnabled>().0,
            "Pressing F should toggle FogOfWarEnabled to false"
        );

        // Press [F] again to toggle Fog of War back ON
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyF);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyF);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        assert!(
            app.world().resource::<FogOfWarEnabled>().0,
            "Pressing F again should toggle FogOfWarEnabled back to true"
        );

        // Assign orders to all groups and verify [X] only clears the currently selected group
        *app.world_mut().resource_mut::<CommanderOrder>() =
            CommanderOrder::TakePosition(Vec2::new(10.0, 25.0));
        app.world_mut().resource_mut::<ApcOrder>().0 =
            CommanderOrder::TakePosition(Vec2::new(-18.0, 60.0));
        app.world_mut().resource_mut::<HelicopterOrder>().0 =
            CommanderOrder::TakePosition(Vec2::new(-25.0, 12.0));

        // Switch to Group 2 (APC) via [E] key
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyE);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        assert_eq!(
            *app.world().resource::<CommandGroup>(),
            CommandGroup::Apc,
            "Pressing E should select CommandGroup::Apc"
        );

        // Press [X] while APC group is selected -> clears ApcOrder to Hold, leaves Infantry Squad & Helicopter orders intact!
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyX);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyX);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        assert_eq!(
            app.world().resource::<ApcOrder>().0,
            CommanderOrder::Hold,
            "Pressing X with APC group selected should reset ApcOrder to Hold"
        );
        assert_eq!(
            *app.world().resource::<CommanderOrder>(),
            CommanderOrder::TakePosition(Vec2::new(10.0, 25.0)),
            "Infantry Squad order must remain untouched when clearing APC order"
        );
        assert_eq!(
            app.world().resource::<HelicopterOrder>().0,
            CommanderOrder::TakePosition(Vec2::new(-25.0, 12.0)),
            "Helicopter order must remain untouched when clearing APC order"
        );

        // Press [Tab] to cycle to Group 3 (Helicopter) and press [X] to clear Helicopter order
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Tab);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::Tab);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        assert_eq!(
            *app.world().resource::<CommandGroup>(),
            CommandGroup::Helicopter,
            "Pressing Tab from Apc should cycle to CommandGroup::Helicopter"
        );

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyX);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyX);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        assert_eq!(
            app.world().resource::<HelicopterOrder>().0,
            CommanderOrder::Hold,
            "Pressing X with Helicopter selected should reset HelicopterOrder to Hold"
        );

        // Press [Tab] to cycle back to Group 1 (Infantry Squad) and press [X] to clear Infantry Squad order
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Tab);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::Tab);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        assert_eq!(
            *app.world().resource::<CommandGroup>(),
            CommandGroup::InfantrySquad,
            "Pressing Tab from Helicopter should cycle back to CommandGroup::InfantrySquad"
        );

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyX);
        app.update();
        assert_eq!(
            *app.world().resource::<CommanderOrder>(),
            CommanderOrder::Hold,
            "Pressing X with InfantrySquad selected should reset CommanderOrder to Hold"
        );
    }
}
