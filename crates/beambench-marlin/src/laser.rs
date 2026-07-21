//! Pure Marlin laser-command construction.
//!
//! Marlin standard mode synchronizes the planner before applying `M3`/`M5`.
//! Inline modes instead attach power to subsequent motion blocks. The `I`
//! parameter is therefore significant: unlike GRBL, bare `M4` is not enough to
//! select dynamic laser power.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MARLIN_LASER_OFF_COMMAND: &str = "M5";
pub const MARLIN_FINISH_MOVES_COMMAND: &str = "M400";

/// Marlin's laser power application mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarlinLaserMode {
    /// Backward-compatible mode. `M3` synchronizes before changing power.
    #[default]
    Standard,
    /// Continuous inline mode selected by `M3 I`.
    ContinuousInline,
    /// Feed-rate-adjusted inline mode selected by `M4 I`.
    DynamicInline,
}

impl MarlinLaserMode {
    const fn activation_prefix(self) -> &'static str {
        match self {
            Self::Standard => "M3",
            Self::ContinuousInline => "M3 I",
            Self::DynamicInline => "M4 I",
        }
    }

    const fn boundary_off_command(self) -> &'static str {
        match self {
            Self::Standard => MARLIN_LASER_OFF_COMMAND,
            Self::ContinuousInline | Self::DynamicInline => "M5 I",
        }
    }
}

/// The configured Marlin `CUTTER_POWER_UNIT` scale used by `S` words.
///
/// `Custom` preserves an escape hatch for vendor builds whose configured range
/// does not match one of Marlin's documented choices.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MarlinPowerScale {
    /// `PWM255`: S0 through S255.
    #[default]
    Pwm255,
    /// `PERCENT`: S0 through S100.
    Percent,
    /// `RPM`: S0 through the machine's configured maximum RPM.
    Rpm { maximum: u32 },
    /// `SERVO`: S0 through S180.
    Servo,
    /// Vendor-specific scale with an explicit maximum.
    Custom { maximum: u32 },
}

impl MarlinPowerScale {
    pub const fn maximum(self) -> u32 {
        match self {
            Self::Pwm255 => 255,
            Self::Percent => 100,
            Self::Rpm { maximum } | Self::Custom { maximum } => maximum,
            Self::Servo => 180,
        }
    }

    fn validate(self) -> Result<(), MarlinLaserCommandError> {
        if self.maximum() == 0 {
            return Err(MarlinLaserCommandError::ZeroPowerMaximum);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum MarlinLaserCommandError {
    #[error("Marlin power maximum must be greater than zero")]
    ZeroPowerMaximum,
    #[error("laser power percent must be finite")]
    NonFinitePowerPercent,
}

/// Validated, transport-neutral Marlin laser command contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarlinLaserCommands {
    mode: MarlinLaserMode,
    power_scale: MarlinPowerScale,
}

impl MarlinLaserCommands {
    pub fn new(
        mode: MarlinLaserMode,
        power_scale: MarlinPowerScale,
    ) -> Result<Self, MarlinLaserCommandError> {
        power_scale.validate()?;
        Ok(Self { mode, power_scale })
    }

    pub const fn mode(&self) -> MarlinLaserMode {
        self.mode
    }

    pub const fn power_scale(&self) -> MarlinPowerScale {
        self.power_scale
    }

    /// Map Beam Bench's 0-100 power percentage onto the configured Marlin
    /// `S` range. Finite out-of-range values clamp like the existing GRBL
    /// generator; malformed non-finite values are rejected.
    pub fn power_value(&self, power_percent: f64) -> Result<u32, MarlinLaserCommandError> {
        if !power_percent.is_finite() {
            return Err(MarlinLaserCommandError::NonFinitePowerPercent);
        }
        let fraction = power_percent.clamp(0.0, 100.0) / 100.0;
        Ok((fraction * f64::from(self.power_scale.maximum())).round() as u32)
    }

    /// Build the mode-selecting laser-power command.
    pub fn laser_on_command(&self, power_percent: f64) -> Result<String, MarlinLaserCommandError> {
        let power = self.power_value(power_percent)?;
        Ok(format!("{} S{power}", self.mode.activation_prefix()))
    }

    /// Build an `S` word for inline `G1`...`G5` motion.
    pub fn motion_power_word(&self, power_percent: f64) -> Result<String, MarlinLaserCommandError> {
        Ok(format!("S{}", self.power_value(power_percent)?))
    }

    /// Turn laser output off without changing the selected inline mode.
    pub const fn laser_off_command(&self) -> &'static str {
        MARLIN_LASER_OFF_COMMAND
    }

    /// Commands for either side of a job boundary.
    ///
    /// Inline contracts use `M5 I` so a completed job does not leak its modal
    /// state into later commands. `M400` provides an explicit
    /// completion barrier even though Marlin's `M5` family also synchronizes.
    pub fn boundary_commands(&self) -> [&'static str; 2] {
        [
            self.mode.boundary_off_command(),
            MARLIN_FINISH_MOVES_COMMAND,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_power_scales_map_percentages() {
        let cases = [
            (MarlinPowerScale::Pwm255, 100.0, 255),
            (MarlinPowerScale::Percent, 50.0, 50),
            (MarlinPowerScale::Rpm { maximum: 30_000 }, 20.0, 6_000),
            (MarlinPowerScale::Servo, 50.0, 90),
            (MarlinPowerScale::Custom { maximum: 1_000 }, 75.0, 750),
        ];

        for (scale, percent, expected) in cases {
            let commands = MarlinLaserCommands::new(MarlinLaserMode::Standard, scale).unwrap();
            assert_eq!(commands.power_value(percent).unwrap(), expected);
        }
    }

    #[test]
    fn mode_selection_keeps_marlin_inline_semantics_explicit() {
        let scale = MarlinPowerScale::Pwm255;
        let command_for = |mode| {
            MarlinLaserCommands::new(mode, scale)
                .unwrap()
                .laser_on_command(80.0)
                .unwrap()
        };

        assert_eq!(command_for(MarlinLaserMode::Standard), "M3 S204");
        assert_eq!(command_for(MarlinLaserMode::ContinuousInline), "M3 I S204");
        assert_eq!(command_for(MarlinLaserMode::DynamicInline), "M4 I S204");
    }

    #[test]
    fn job_boundaries_turn_output_off_and_wait_for_completion() {
        let standard =
            MarlinLaserCommands::new(MarlinLaserMode::Standard, MarlinPowerScale::Pwm255).unwrap();
        let inline =
            MarlinLaserCommands::new(MarlinLaserMode::DynamicInline, MarlinPowerScale::Pwm255)
                .unwrap();

        assert_eq!(standard.laser_off_command(), "M5");
        assert_eq!(standard.boundary_commands(), ["M5", "M400"]);
        assert_eq!(inline.laser_off_command(), "M5");
        assert_eq!(inline.boundary_commands(), ["M5 I", "M400"]);
    }

    #[test]
    fn finite_percentages_clamp_but_malformed_values_fail() {
        let commands =
            MarlinLaserCommands::new(MarlinLaserMode::Standard, MarlinPowerScale::Percent).unwrap();

        assert_eq!(commands.power_value(-10.0).unwrap(), 0);
        assert_eq!(commands.power_value(110.0).unwrap(), 100);
        assert_eq!(
            commands.power_value(f64::NAN).unwrap_err(),
            MarlinLaserCommandError::NonFinitePowerPercent
        );
    }

    #[test]
    fn zero_sized_configured_scales_are_rejected() {
        for scale in [
            MarlinPowerScale::Rpm { maximum: 0 },
            MarlinPowerScale::Custom { maximum: 0 },
        ] {
            assert_eq!(
                MarlinLaserCommands::new(MarlinLaserMode::Standard, scale).unwrap_err(),
                MarlinLaserCommandError::ZeroPowerMaximum
            );
        }
    }
}
