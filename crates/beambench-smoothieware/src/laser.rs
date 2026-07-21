//! Pure Smoothieware laser-command construction.
//!
//! Smoothieware's laser module reads modal `S` values from motion blocks. It
//! does not use GRBL's `M3`/`M4` laser-mode contract. `M221 P0` enables the
//! firmware's speed-proportional behavior and `M221 P1` disables it.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SMOOTHIEWARE_FINISH_MOVES_COMMAND: &str = "M400";
pub const SMOOTHIEWARE_CANCEL_COMMAND: &str = "M112";

/// How Smoothieware applies requested power while a laser motion block runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmoothiewarePowerMode {
    /// Scale requested power with the block's actual speed.
    #[default]
    SpeedProportional,
    /// Hold requested power through acceleration and deceleration.
    Constant,
}

/// The configured `laser_module_maximum_s_value` represented by 100% power.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SmoothiewarePowerScale {
    #[serde(default = "default_maximum_s_value")]
    pub maximum_s_value: f64,
}

const fn default_maximum_s_value() -> f64 {
    1.0
}

impl Default for SmoothiewarePowerScale {
    fn default() -> Self {
        Self {
            maximum_s_value: default_maximum_s_value(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum SmoothiewareLaserCommandError {
    #[error("Smoothieware maximum S value must be finite and greater than zero")]
    InvalidPowerMaximum,
    #[error("laser power percent must be finite")]
    NonFinitePowerPercent,
}

/// Validated, transport-neutral Smoothieware laser command contract.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothiewareLaserCommands {
    mode: SmoothiewarePowerMode,
    power_scale: SmoothiewarePowerScale,
}

impl SmoothiewareLaserCommands {
    pub fn new(
        mode: SmoothiewarePowerMode,
        power_scale: SmoothiewarePowerScale,
    ) -> Result<Self, SmoothiewareLaserCommandError> {
        if !power_scale.maximum_s_value.is_finite() || power_scale.maximum_s_value <= 0.0 {
            return Err(SmoothiewareLaserCommandError::InvalidPowerMaximum);
        }
        Ok(Self { mode, power_scale })
    }

    pub const fn mode(&self) -> SmoothiewarePowerMode {
        self.mode
    }

    pub const fn power_scale(&self) -> SmoothiewarePowerScale {
        self.power_scale
    }

    /// Explicitly select the firmware's speed-power behavior and restore its
    /// runtime scale multiplier to 100% at the beginning of every job.
    pub const fn mode_command(&self) -> &'static str {
        match self.mode {
            SmoothiewarePowerMode::SpeedProportional => "M221 S100 P0",
            SmoothiewarePowerMode::Constant => "M221 S100 P1",
        }
    }

    /// Map Beam Bench's 0-100 percentage to the configured Smoothieware range.
    pub fn power_value(&self, power_percent: f64) -> Result<f64, SmoothiewareLaserCommandError> {
        if !power_percent.is_finite() {
            return Err(SmoothiewareLaserCommandError::NonFinitePowerPercent);
        }
        Ok(power_percent.clamp(0.0, 100.0) / 100.0 * self.power_scale.maximum_s_value)
    }

    /// Build the explicit `S` word attached to a generated feed move.
    pub fn motion_power_word(
        &self,
        power_percent: f64,
    ) -> Result<String, SmoothiewareLaserCommandError> {
        Ok(format_s_word(self.power_value(power_percent)?))
    }
}

pub(crate) fn format_s_word(value: f64) -> String {
    let mut rendered = format!("{value:.6}");
    while rendered.ends_with('0') {
        rendered.pop();
    }
    if rendered.ends_with('.') {
        rendered.pop();
    }
    if rendered == "-0" {
        rendered = "0".to_string();
    }
    format!("S{rendered}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_scale_matches_smoothieware_firmware_default() {
        let commands = SmoothiewareLaserCommands::new(
            SmoothiewarePowerMode::SpeedProportional,
            SmoothiewarePowerScale::default(),
        )
        .unwrap();
        assert_eq!(commands.motion_power_word(0.0).unwrap(), "S0");
        assert_eq!(commands.motion_power_word(50.0).unwrap(), "S0.5");
        assert_eq!(commands.motion_power_word(100.0).unwrap(), "S1");
    }

    #[test]
    fn configured_scale_and_mode_are_explicit() {
        let commands = SmoothiewareLaserCommands::new(
            SmoothiewarePowerMode::Constant,
            SmoothiewarePowerScale {
                maximum_s_value: 255.0,
            },
        )
        .unwrap();
        assert_eq!(commands.mode_command(), "M221 S100 P1");
        assert_eq!(commands.motion_power_word(50.0).unwrap(), "S127.5");
    }

    #[test]
    fn malformed_values_fail_and_finite_percentages_clamp() {
        for maximum_s_value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                SmoothiewareLaserCommands::new(
                    SmoothiewarePowerMode::default(),
                    SmoothiewarePowerScale { maximum_s_value },
                )
                .unwrap_err(),
                SmoothiewareLaserCommandError::InvalidPowerMaximum
            );
        }

        let commands = SmoothiewareLaserCommands::new(
            SmoothiewarePowerMode::default(),
            SmoothiewarePowerScale::default(),
        )
        .unwrap();
        assert_eq!(commands.motion_power_word(-10.0).unwrap(), "S0");
        assert_eq!(commands.motion_power_word(110.0).unwrap(), "S1");
        assert_eq!(
            commands.motion_power_word(f64::NAN).unwrap_err(),
            SmoothiewareLaserCommandError::NonFinitePowerPercent
        );
    }
}
