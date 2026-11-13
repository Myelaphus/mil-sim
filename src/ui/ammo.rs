use bevy::prelude::*;
use crate::common::types::{LevelEntity, Soldier, MAG_CAPACITY};
use crate::player::Player;

#[derive(Component)]
pub struct AmmoHudRoot;

#[derive(Component)]
pub struct EquippedWeaponText;

#[derive(Component)]
pub struct WeaponCapacityText;

#[derive(Component)]
pub struct AmmoCountText;

#[derive(Component)]
pub struct AmmoReserveText;

#[derive(Component)]
pub struct ReloadStatusText;

#[derive(Component)]
pub struct ReloadProgressBar;

#[derive(Component)]
pub struct ReloadProgressBarContainer;

// Spawns clean tactical HUD element in bottom-right corner for ammo, magazines, and reload status
pub fn setup_ammo_ui(mut commands: Commands) {
    commands
        .spawn((
            AmmoHudRoot,
            LevelEntity,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(22.0),
                right: Val::Px(24.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(16.0), Val::Px(12.0)),
                row_gap: Val::Px(5.0),
                border: UiRect::all(Val::Px(1.5)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                min_width: Val::Px(230.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.07, 0.10, 0.14, 0.88)),
            BorderColor::all(Color::srgb(0.22, 0.34, 0.48)),
        ))
        .with_children(|root| {
            // Weapon header
            root.spawn((
                EquippedWeaponText,
                Text::new("SERVICE RIFLE  •  5.56mm"),
                TextFont {
                    font_size: FontSize::Px(11.0),
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.65, 0.78)),
            ));

            // Main ammo counter readout row
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Baseline,
                    column_gap: Val::Px(8.0),
                    ..default()
                },
            ))
            .with_children(|row| {
                // Large digital magazine count
                row.spawn((
                    AmmoCountText,
                    Text::new("30"),
                    TextFont {
                        font_size: FontSize::Px(32.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.95, 0.98, 1.0)),
                ));

                // Magazine max capacity separator
                row.spawn((
                    WeaponCapacityText,
                    Text::new(format!("/ {}", MAG_CAPACITY)),
                    TextFont {
                        font_size: FontSize::Px(16.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.48, 0.58, 0.68)),
                ));

                // Reserve magazines display
                row.spawn((
                    AmmoReserveText,
                    Text::new("MAGS: 6"),
                    TextFont {
                        font_size: FontSize::Px(14.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.68, 0.80, 0.94)),
                    Node {
                        margin: UiRect::left(Val::Px(14.0)),
                        ..default()
                    },
                ));
            });

            // Reload / weapon swap progress bar container
            root.spawn((
                ReloadProgressBarContainer,
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(5.0),
                    border_radius: BorderRadius::all(Val::Px(2.5)),
                    margin: UiRect::top(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.14, 0.18, 0.24, 0.9)),
            ))
            .with_children(|bar_container| {
                bar_container.spawn((
                    ReloadProgressBar,
                    Node {
                        width: Val::Percent(0.0),
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(2.5)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(1.0, 0.72, 0.20)),
                ));
            });

            // Reload status indicator text
            root.spawn((
                ReloadStatusText,
                Text::new("[R] RELOAD  |  [F] SWAP"),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(Color::srgb(0.48, 0.58, 0.68)),
            ));
        });
}

// Updates player ammunition readout, reload progress bar, and status messages
pub fn update_ammo_ui(
    player_q: Query<&Soldier, With<Player>>,
    mut weapon_name_q: Query<
        &mut Text,
        (
            With<EquippedWeaponText>,
            Without<AmmoCountText>,
            Without<AmmoReserveText>,
            Without<ReloadStatusText>,
            Without<WeaponCapacityText>,
        ),
    >,
    mut capacity_q: Query<
        &mut Text,
        (
            With<WeaponCapacityText>,
            Without<AmmoCountText>,
            Without<AmmoReserveText>,
            Without<ReloadStatusText>,
            Without<EquippedWeaponText>,
        ),
    >,
    mut ammo_count_q: Query<
        (&mut Text, &mut TextColor),
        (
            With<AmmoCountText>,
            Without<AmmoReserveText>,
            Without<ReloadStatusText>,
            Without<EquippedWeaponText>,
            Without<WeaponCapacityText>,
        ),
    >,
    mut reserve_q: Query<
        &mut Text,
        (
            With<AmmoReserveText>,
            Without<AmmoCountText>,
            Without<ReloadStatusText>,
            Without<EquippedWeaponText>,
            Without<WeaponCapacityText>,
        ),
    >,
    mut status_q: Query<
        (&mut Text, &mut TextColor),
        (
            With<ReloadStatusText>,
            Without<AmmoCountText>,
            Without<AmmoReserveText>,
            Without<EquippedWeaponText>,
            Without<WeaponCapacityText>,
        ),
    >,
    mut bar_q: Query<(&mut Node, &mut BackgroundColor), With<ReloadProgressBar>>,
) {
    let Ok(player_soldier) = player_q.single() else {
        // Player is dead / not in game
        for mut text in &mut weapon_name_q {
            **text = "NO WEAPON".to_string();
        }
        for mut text in &mut capacity_q {
            **text = "/ --".to_string();
        }
        for (mut text, mut color) in &mut ammo_count_q {
            **text = "--".to_string();
            color.0 = Color::srgb(0.5, 0.5, 0.5);
        }
        for mut text in &mut reserve_q {
            **text = "MAGS: --".to_string();
        }
        for (mut text, mut color) in &mut status_q {
            **text = "K.I.A.".to_string();
            color.0 = Color::srgb(0.85, 0.25, 0.25);
        }
        for (mut node, _) in &mut bar_q {
            node.width = Val::Percent(0.0);
        }
        return;
    };

    let inventory = &player_soldier.inventory;
    let Some(active_weapon) = inventory.active_weapon() else {
        return;
    };

    // 1. Equipped weapon name & capacity
    for mut text in &mut weapon_name_q {
        **text = active_weapon.weapon_type.display_name().to_string();
    }
    for mut text in &mut capacity_q {
        **text = format!("/ {}", active_weapon.weapon_type.mag_capacity());
    }

    let ammo = &active_weapon.ammo;

    // 2. Current magazine count & color coding
    for (mut text, mut color) in &mut ammo_count_q {
        **text = format!("{}", ammo.current_mag);
        if ammo.current_mag == 0 {
            color.0 = Color::srgb(0.95, 0.25, 0.25); // Red warning when empty
        } else if ammo.current_mag <= 7 {
            color.0 = Color::srgb(1.0, 0.72, 0.20); // Amber warning when low
        } else {
            color.0 = Color::srgb(0.95, 0.98, 1.0); // White when normal
        }
    }

    // 3. Reserve magazines
    for mut text in &mut reserve_q {
        **text = format!("MAGS: {}", ammo.reserve_mags);
    }

    // 4. Status and progress bar (handling weapon swap and reload indicators)
    let is_swapping = inventory.is_swapping();
    let is_reloading = ammo.is_reloading();

    if is_swapping {
        let progress = inventory.swap_progress().unwrap_or(0.0);
        let remaining_secs = inventory.swap_remaining_secs().unwrap_or(0.0);
        let target_name = inventory
            .pending_weapon()
            .map(|w| w.weapon_type.name())
            .unwrap_or("WEAPON");

        for (mut node, mut bg) in &mut bar_q {
            node.width = Val::Percent((progress * 100.0).clamp(0.0, 100.0));
            bg.0 = Color::srgb(0.35, 0.75, 1.0); // Cyan progress bar for weapon swap
        }

        for (mut text, mut color) in &mut status_q {
            **text = format!("⟳ SWAPPING TO {}... {:.1}s", target_name, remaining_secs);
            color.0 = Color::srgb(0.35, 0.75, 1.0);
        }
    } else if is_reloading {
        let progress = ammo.reload_progress().unwrap_or(0.0);
        let remaining_secs = ammo.reload_remaining_secs().unwrap_or(0.0);

        for (mut node, mut bg) in &mut bar_q {
            node.width = Val::Percent((progress * 100.0).clamp(0.0, 100.0));
            bg.0 = Color::srgb(1.0, 0.72, 0.20); // Amber progress bar for reload
        }

        for (mut text, mut color) in &mut status_q {
            **text = format!("⟳ RELOADING... {:.1}s", remaining_secs);
            color.0 = Color::srgb(1.0, 0.72, 0.20);
        }
    } else {
        for (mut node, _) in &mut bar_q {
            node.width = Val::Percent(0.0);
        }

        let has_swap = inventory.weapons.len() > 1;
        for (mut text, mut color) in &mut status_q {
            if ammo.current_mag == 0 && ammo.reserve_mags == 0 {
                if has_swap {
                    **text = "⚠ OUT OF AMMO • [F] SWAP".to_string();
                } else {
                    **text = "⚠ OUT OF AMMO".to_string();
                }
                color.0 = Color::srgb(0.95, 0.25, 0.25);
            } else if ammo.current_mag < active_weapon.weapon_type.mag_capacity() && ammo.reserve_mags > 0 {
                if has_swap {
                    **text = "[R] RELOAD  |  [F] SWAP".to_string();
                } else {
                    **text = "[R] RELOAD".to_string();
                }
                color.0 = Color::srgb(0.48, 0.58, 0.68);
            } else {
                if has_swap {
                    **text = "READY  |  [F] SWAP".to_string();
                } else {
                    **text = "READY".to_string();
                }
                color.0 = Color::srgb(0.35, 0.75, 0.45);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ammo_ui_updates_from_player_state() {
        let mut app = App::new();

        let mut soldier = Soldier::default();
        let weapon = soldier.inventory.active_weapon_mut().unwrap();
        weapon.ammo.current_mag = 18;
        weapon.ammo.reserve_mags = 4;

        app.world_mut().spawn((
            Player,
            soldier,
        ));

        let weapon_text_e = app
            .world_mut()
            .spawn((
                EquippedWeaponText,
                Text::new(""),
            ))
            .id();

        let capacity_text_e = app
            .world_mut()
            .spawn((
                WeaponCapacityText,
                Text::new(""),
            ))
            .id();

        let ammo_text_e = app
            .world_mut()
            .spawn((
                AmmoCountText,
                Text::new("30"),
                TextColor(Color::WHITE),
            ))
            .id();

        let reserve_text_e = app
            .world_mut()
            .spawn((
                AmmoReserveText,
                Text::new("MAGS: 6"),
            ))
            .id();

        let status_text_e = app
            .world_mut()
            .spawn((
                ReloadStatusText,
                Text::new(""),
                TextColor(Color::WHITE),
            ))
            .id();

        let bar_e = app
            .world_mut()
            .spawn((
                ReloadProgressBar,
                Node {
                    width: Val::Percent(0.0),
                    ..default()
                },
                BackgroundColor(Color::WHITE),
            ))
            .id();

        app.add_systems(Update, update_ammo_ui);
        app.update();

        let weapon_t = app.world().get::<Text>(weapon_text_e).unwrap();
        assert_eq!(weapon_t.0.as_str(), "SERVICE RIFLE  •  5.56mm");

        let capacity_t = app.world().get::<Text>(capacity_text_e).unwrap();
        assert_eq!(capacity_t.0.as_str(), "/ 30");

        let ammo_t = app.world().get::<Text>(ammo_text_e).unwrap();
        assert_eq!(ammo_t.0.as_str(), "18");

        let reserve_t = app.world().get::<Text>(reserve_text_e).unwrap();
        assert_eq!(reserve_t.0.as_str(), "MAGS: 4");

        let status_t = app.world().get::<Text>(status_text_e).unwrap();
        assert_eq!(status_t.0.as_str(), "[R] RELOAD");

        let bar_node = app.world().get::<Node>(bar_e).unwrap();
        assert_eq!(bar_node.width, Val::Percent(0.0));
    }

    #[test]
    fn test_ammo_ui_shows_reloading_state() {
        let mut app = App::new();

        let mut soldier = Soldier::default();
        let weapon = soldier.inventory.active_weapon_mut().unwrap();
        weapon.ammo.current_mag = 0;
        weapon.ammo.start_reload();

        app.world_mut().spawn((
            Player,
            soldier,
        ));

        let _weapon_text_e = app
            .world_mut()
            .spawn((
                EquippedWeaponText,
                Text::new(""),
            ))
            .id();

        let _capacity_text_e = app
            .world_mut()
            .spawn((
                WeaponCapacityText,
                Text::new(""),
            ))
            .id();

        let ammo_text_e = app
            .world_mut()
            .spawn((
                AmmoCountText,
                Text::new("30"),
                TextColor(Color::WHITE),
            ))
            .id();

        let reserve_text_e = app
            .world_mut()
            .spawn((
                AmmoReserveText,
                Text::new("MAGS: 6"),
            ))
            .id();

        let status_text_e = app
            .world_mut()
            .spawn((
                ReloadStatusText,
                Text::new(""),
                TextColor(Color::WHITE),
            ))
            .id();

        let _bar_e = app
            .world_mut()
            .spawn((
                ReloadProgressBar,
                Node {
                    width: Val::Percent(0.0),
                    ..default()
                },
                BackgroundColor(Color::WHITE),
            ))
            .id();

        app.add_systems(Update, update_ammo_ui);
        app.update();

        let ammo_t = app.world().get::<Text>(ammo_text_e).unwrap();
        let ammo_c = app.world().get::<TextColor>(ammo_text_e).unwrap();
        assert_eq!(ammo_t.0.as_str(), "0");
        assert_eq!(ammo_c.0, Color::srgb(0.95, 0.25, 0.25)); // Red when empty

        let status_t = app.world().get::<Text>(status_text_e).unwrap();
        let status_c = app.world().get::<TextColor>(status_text_e).unwrap();
        assert!(status_t.0.starts_with("⟳ RELOADING..."));
        assert_eq!(status_c.0, Color::srgb(1.0, 0.72, 0.20)); // Amber warning

        let reserve_t = app.world().get::<Text>(reserve_text_e).unwrap();
        assert_eq!(reserve_t.0.as_str(), "MAGS: 6");
    }

    #[test]
    fn test_ammo_ui_shows_weapon_swapping_state() {
        let mut app = App::new();

        let mut soldier = Soldier::default().with_inventory(crate::common::types::Inventory::player_default());
        soldier.inventory.start_swap();

        app.world_mut().spawn((
            Player,
            soldier,
        ));

        let weapon_text_e = app
            .world_mut()
            .spawn((
                EquippedWeaponText,
                Text::new(""),
            ))
            .id();

        let capacity_text_e = app
            .world_mut()
            .spawn((
                WeaponCapacityText,
                Text::new(""),
            ))
            .id();

        let _ammo_text_e = app
            .world_mut()
            .spawn((
                AmmoCountText,
                Text::new("30"),
                TextColor(Color::WHITE),
            ))
            .id();

        let _reserve_text_e = app
            .world_mut()
            .spawn((
                AmmoReserveText,
                Text::new("MAGS: 6"),
            ))
            .id();

        let status_text_e = app
            .world_mut()
            .spawn((
                ReloadStatusText,
                Text::new(""),
                TextColor(Color::WHITE),
            ))
            .id();

        let bar_e = app
            .world_mut()
            .spawn((
                ReloadProgressBar,
                Node {
                    width: Val::Percent(0.0),
                    ..default()
                },
                BackgroundColor(Color::WHITE),
            ))
            .id();

        app.add_systems(Update, update_ammo_ui);
        app.update();

        let status_t = app.world().get::<Text>(status_text_e).unwrap();
        let status_c = app.world().get::<TextColor>(status_text_e).unwrap();
        assert!(status_t.0.starts_with("⟳ SWAPPING TO SIDEARM PISTOL..."));
        assert_eq!(status_c.0, Color::srgb(0.35, 0.75, 1.0)); // Cyan swap indicator

        let weapon_t = app.world().get::<Text>(weapon_text_e).unwrap();
        assert_eq!(weapon_t.0.as_str(), "SERVICE RIFLE  •  5.56mm");
        let cap_t = app.world().get::<Text>(capacity_text_e).unwrap();
        assert_eq!(cap_t.0.as_str(), "/ 30");
        let bar_node = app.world().get::<Node>(bar_e).unwrap();
        assert_eq!(bar_node.width, Val::Percent(0.0));
    }
}
