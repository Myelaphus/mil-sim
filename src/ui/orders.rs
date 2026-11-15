use crate::common::types::{
    CommanderOrder, CurrentLevel, LevelEntity, OrderType, RTS_SOLDIER_WORLD_BOUND,
    SelectedOrderTool,
};
use bevy::prelude::*;
use std::f32::consts::FRAC_PI_2;

#[derive(Component)]
pub struct OrdersHudRoot;

#[derive(Component)]
pub struct TakePositionOrderButton;

#[derive(Component)]
pub struct ClearOrderButton;

#[derive(Component)]
pub struct OrderStatusText;

#[derive(Component)]
pub struct OrderObjectiveMarker;

pub const ORDER_BTN_NORMAL: Color = Color::srgb(0.13, 0.18, 0.24);
pub const ORDER_BTN_HOVER: Color = Color::srgb(0.22, 0.32, 0.44);
pub const ORDER_BTN_SELECTED: Color = Color::srgb(0.14, 0.56, 0.42);
pub const CLEAR_BTN_NORMAL: Color = Color::srgb(0.24, 0.14, 0.16);
pub const CLEAR_BTN_HOVER: Color = Color::srgb(0.36, 0.20, 0.22);

/// Spawns the RTS Commander Orders toolbar and 3D ground objective marker for `test_level_4`.
pub fn setup_orders_ui(
    mut commands: Commands,
    current_level: Option<Res<CurrentLevel>>,
    mut selected_tool: Option<ResMut<SelectedOrderTool>>,
    mut commander_order: Option<ResMut<CommanderOrder>>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    if let Some(ref mut tool) = selected_tool {
        tool.0 = None;
    }
    if let Some(ref mut order) = commander_order {
        **order = CommanderOrder::Hold;
    }

    if current_level.as_deref().copied().unwrap_or_default() != CurrentLevel::Level4 {
        return;
    }

    // Spawn 3D ground marker for active objective position (hidden when Hold)
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
                Mesh3d(outer_mesh),
                MeshMaterial3d(outer_mat),
                Transform::from_xyz(0.0, 0.02, 0.0)
                    .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                Visibility::Hidden,
            ))
            .id();

        commands.spawn((
            Mesh3d(inner_mesh),
            MeshMaterial3d(inner_mat),
            Transform::from_xyz(0.0, 0.0, 0.002),
            ChildOf(marker),
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
                Text::new("COMMANDER ORDERS  •  GENERAL INTENT"),
                TextFont {
                    font_size: FontSize::Px(11.0),
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.68, 0.82)),
            ));

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
        });
}

/// Handles order type selection (via UI buttons or hotkeys) and map click target assignment.
pub fn update_orders_ui(
    current_level: Option<Res<CurrentLevel>>,
    keyboard_input: Option<Res<ButtonInput<KeyCode>>>,
    mouse_buttons: Option<Res<ButtonInput<MouseButton>>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    selected_tool: Option<ResMut<SelectedOrderTool>>,
    commander_order: Option<ResMut<CommanderOrder>>,
    mut take_pos_btn_q: Query<
        (&Interaction, &mut BackgroundColor),
        (With<TakePositionOrderButton>, Without<ClearOrderButton>),
    >,
    mut clear_btn_q: Query<
        (&Interaction, &mut BackgroundColor),
        (With<ClearOrderButton>, Without<TakePositionOrderButton>),
    >,
    mut status_text_q: Query<(&mut Text, &mut TextColor), With<OrderStatusText>>,
    mut marker_q: Query<
        (&mut Transform, &mut Visibility),
        (With<OrderObjectiveMarker>, Without<Camera3d>),
    >,
) {
    if current_level.as_deref().copied().unwrap_or_default() != CurrentLevel::Level4 {
        return;
    }

    let (Some(mut selected_tool), Some(mut commander_order)) = (selected_tool, commander_order)
    else {
        return;
    };

    let mut ui_captured_click = false;

    // 1. UI button interactions for selecting/clearing order types
    for (interaction, mut bg) in &mut take_pos_btn_q {
        match *interaction {
            Interaction::Pressed => {
                ui_captured_click = true;
                selected_tool.0 = if selected_tool.0 == Some(OrderType::TakePosition) {
                    None
                } else {
                    Some(OrderType::TakePosition)
                };
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

    for (interaction, mut bg) in &mut clear_btn_q {
        match *interaction {
            Interaction::Pressed => {
                ui_captured_click = true;
                selected_tool.0 = None;
                *commander_order = CommanderOrder::Hold;
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

    // 2. Keyboard shortcuts for selecting order types or cancelling
    if let Some(ref keys) = keyboard_input {
        if keys.just_pressed(KeyCode::Digit1) {
            selected_tool.0 = if selected_tool.0 == Some(OrderType::TakePosition) {
                None
            } else {
                Some(OrderType::TakePosition)
            };
        }
        if keys.just_pressed(KeyCode::Escape) {
            selected_tool.0 = None;
        }
        if keys.just_pressed(KeyCode::KeyX) || keys.just_pressed(KeyCode::Backspace) {
            selected_tool.0 = None;
            *commander_order = CommanderOrder::Hold;
        }
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

    // 3. When an OrderType is armed and the user Left-Clicks on the map, issue the order
    if let Some(active_tool) = selected_tool.0 {
        let left_clicked = mouse_buttons
            .as_ref()
            .is_some_and(|mb| mb.just_pressed(MouseButton::Left));
        if left_clicked && !ui_captured_click {
            if let (Ok(window), Ok((camera, cam_global_tf))) =
                (windows.single(), camera_q.single())
            {
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
                        match active_tool {
                            OrderType::TakePosition => {
                                *commander_order = CommanderOrder::TakePosition(clamped_target);
                            }
                        }
                        selected_tool.0 = None;
                    }
                }
            }
        }
    }

    // 4. Update HUD status readout and 3D objective ground marker
    for (mut text, mut color) in &mut status_text_q {
        if let Some(OrderType::TakePosition) = selected_tool.0 {
            **text =
                "ORDER ARMED: TAKE POSITION  •  Left-click map to assign target  ([ESC] Cancel)"
                    .to_string();
            *color = TextColor(Color::srgb(0.32, 0.95, 0.68));
        } else {
            match *commander_order {
                CommanderOrder::Hold => {
                    **text = "ACTIVE ORDER: HOLD  •  Select an order type ([1])".to_string();
                    *color = TextColor(Color::srgb(0.75, 0.84, 0.92));
                }
                CommanderOrder::TakePosition(pos) => {
                    **text = format!(
                        "ACTIVE ORDER: TAKE POSITION ({:.0}, {:.0})  •  Select [1] to reassign or [X] Hold",
                        pos.x, pos.y
                    );
                    *color = TextColor(Color::srgb(0.35, 0.88, 1.0));
                }
            }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_type_selection_and_clear_via_buttons_and_hotkeys() {
        let mut app = App::new();
        app.insert_resource(CurrentLevel::Level4);
        app.init_resource::<SelectedOrderTool>();
        app.init_resource::<CommanderOrder>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<ButtonInput<MouseButton>>();

        app.add_systems(Startup, setup_orders_ui);
        app.add_systems(Update, update_orders_ui);
        app.update();

        // Initially no tool is armed and order is Hold
        assert_eq!(app.world().resource::<SelectedOrderTool>().0, None);
        assert_eq!(*app.world().resource::<CommanderOrder>(), CommanderOrder::Hold);

        // Press [1] key to select TakePosition order type
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Digit1);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear_just_pressed(KeyCode::Digit1);

        assert_eq!(
            app.world().resource::<SelectedOrderTool>().0,
            Some(OrderType::TakePosition),
            "Pressing Digit1 should arm OrderType::TakePosition"
        );

        // Simulate assigning CommanderOrder::TakePosition and verify marker visibility
        *app.world_mut().resource_mut::<CommanderOrder>() =
            CommanderOrder::TakePosition(Vec2::new(10.0, 25.0));
        app.world_mut().resource_mut::<SelectedOrderTool>().0 = None;
        app.update();

        // Press [X] to clear active order back to Hold
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyX);
        app.update();

        assert_eq!(
            *app.world().resource::<CommanderOrder>(),
            CommanderOrder::Hold,
            "Pressing X should reset CommanderOrder to Hold"
        );
    }
}

