//! On-screen text: crosshair, current weapon/throwable, slow-mo state and controls.

use bevy::prelude::*;

use crate::muscles::MuscleTone;
use crate::physics::SlowMotion;
use crate::player::MouseCaptured;
use crate::throwing::ThrowState;
use crate::weapons::Arsenal;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hud).add_systems(Update, update_hud);
    }
}

#[derive(Component)]
struct StatusText;

const CONTROLS: &str = "\
WASD move | Shift sprint | Space jump | Mouse look
LMB shoot | 1 pistol | 2 shotgun | E explosion at crosshair
Hold RMB charge throw | Q ball/crate
R reset person | F spawn person in front | T slow-mo | Esc free mouse
G muscles on/off | - / = muscle tone";

fn spawn_hud(mut commands: Commands) {
    // Crosshair: a "+" in the middle of the screen.
    commands
        .spawn(Node {
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_child((Text::new("+"), TextFont { font_size: FontSize::Px(28.0), ..default() }));

    commands.spawn((
        StatusText,
        Text::new(""),
        TextFont { font_size: FontSize::Px(16.0), ..default() },
        TextShadow::default(),
        Node {
            position_type: PositionType::Absolute,
            top: px(10),
            left: px(12),
            ..default()
        },
    ));

    commands.spawn((
        Text::new(CONTROLS),
        TextFont { font_size: FontSize::Px(14.0), ..default() },
        TextShadow::default(),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(10),
            left: px(12),
            ..default()
        },
    ));
}

fn update_hud(
    mut text: Single<&mut Text, With<StatusText>>,
    arsenal: Res<Arsenal>,
    throw: Res<ThrowState>,
    slow: Res<SlowMotion>,
    captured: Res<MouseCaptured>,
    tone: Res<MuscleTone>,
) {
    let charge = match throw.charge {
        Some(c) => format!(" [{}{}]", "#".repeat((c * 10.0) as usize), "-".repeat(10 - (c * 10.0) as usize)),
        None => String::new(),
    };
    let mut status = format!(
        "Weapon: {:?}\nThrow: {:?}{}\nMuscle tone: {:.0}%\n{}",
        arsenal.current,
        throw.selected,
        charge,
        tone.0 * 100.0,
        if slow.0 { "SLOW MOTION" } else { "" },
    );
    if !captured.0 {
        status.push_str("\n\nClick the window to start");
    }
    text.0 = status;
}
