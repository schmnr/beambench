use beambench_marlin::{MarlinLaserCommands, MarlinLaserMode, MarlinPowerScale};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Corpus {
    schema_version: u32,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    mode: MarlinLaserMode,
    power_scale: MarlinPowerScale,
    power_percent: f64,
    expected_power_value: u32,
    expected_on_command: String,
    expected_motion_power_word: String,
    expected_boundary_commands: [String; 2],
}

#[test]
fn versioned_marlin_laser_command_corpus_is_stable() {
    let corpus: Corpus =
        serde_json::from_str(include_str!("fixtures/laser_commands.json")).unwrap();
    assert_eq!(corpus.schema_version, 1);
    assert!(!corpus.cases.is_empty());

    for case in corpus.cases {
        let commands = MarlinLaserCommands::new(case.mode, case.power_scale)
            .unwrap_or_else(|error| panic!("{}: {error}", case.name));

        assert_eq!(
            commands.power_value(case.power_percent).unwrap(),
            case.expected_power_value,
            "{} power value",
            case.name
        );
        assert_eq!(
            commands.laser_on_command(case.power_percent).unwrap(),
            case.expected_on_command,
            "{} on command",
            case.name
        );
        assert_eq!(
            commands.motion_power_word(case.power_percent).unwrap(),
            case.expected_motion_power_word,
            "{} motion power",
            case.name
        );
        let boundary_commands = commands.boundary_commands();
        assert_eq!(
            boundary_commands.as_slice(),
            case.expected_boundary_commands.as_slice(),
            "{} boundary commands",
            case.name
        );
    }
}
