use island::model::Settings;

#[test]
fn legacy_material_and_removed_feature_settings_are_retired() {
    let path = std::env::temp_dir().join(format!("island-black-skin-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    std::env::set_var("DAOZAI_DATA_DIR", &path);
    for skin in ["glass", "clear", "black"] {
        std::fs::write(path.join("settings.txt"), format!("material={skin}\npet=1\nwater_minutes=5\nvisible_pages=00100000\n")).unwrap();
        let settings = Settings::load();
        assert!(!settings.pet);
        assert!(!settings.visible_pages[2]);assert!(settings.visible_pages[0]);
        settings.save();
        let saved = std::fs::read_to_string(path.join("settings.txt")).unwrap();
        assert!(!saved.contains("material="));
        assert!(!saved.contains("pet="));assert!(!saved.contains("water_minutes="));
    }
    std::fs::remove_file(path.join("settings.txt")).unwrap();
    std::fs::remove_dir(path).unwrap();
}
