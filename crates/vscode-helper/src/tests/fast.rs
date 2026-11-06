use super::*;

#[test]
#[ignore = "requires EASY_SWITCH_TEST_CODEX; isolated official model/list, no generation"]
fn real_appserver_exposes_fast_catalog() -> Result<()> {
    let executable = PathBuf::from(std::env::var_os("EASY_SWITCH_TEST_CODEX").context("set EASY_SWITCH_TEST_CODEX")?);
    let mut f = Fixture::new();
    let root = f._root.path();
    for dir in ["user", "appdata", "localappdata", "tmp"] { fs::create_dir_all(root.join(dir))?; }
    // The fixture's fake token must never be supplied to the real parser.
    fs::remove_file(f.request.home.join("auth.json"))?;
    f.select("a", "synthetic-fast-model");
    f.request.provider.as_mut().unwrap().native_fast = true;
    f.request.provider.as_mut().unwrap().base_url = "http://127.0.0.1:9/v1".into();
    f.run()?;
    // Let the real binary create its full schema instead of opening the fixture's minimal index.
    fs::remove_file(f.request.home.join("state_5.sqlite"))?;
    let root = f._root.path();
    let evidence = root.join("fast-proof.log");
    let started = appserver::Server::start(&executable, root, "http://127.0.0.1:9", &evidence);
    if started.is_err() { eprintln!("{}", fs::read_to_string(&evidence).unwrap_or_default()); }
    let mut server = started?;
    let models = server.rpc(2, "model/list", json!({"includeHidden":true}))?;
    let model = models["data"].as_array().context("missing models")?.iter().find(|m| m["model"] == "synthetic-fast-model").context("missing synthetic model")?;
    ensure!(model["serviceTiers"].as_array().is_some_and(|tiers| tiers.iter().any(|t| t["id"] == "priority")), "Fast tier missing: {model}");
    let config = server.rpc(3, "config/read", json!({"includeLayers":false,"cwd":null}))?;
    ensure!(config["config"]["model_provider"] == PROVIDER, "wrong provider");
    ensure!(config["config"]["model_catalog_json"].as_str().is_some_and(|p| p.ends_with("fast-models.json")), "catalog path missing from UI config projection");
    ensure!(config["config"]["model_providers"][PROVIDER]["requires_openai_auth"] == false, "provider scope missing from UI config projection");
    println!("PASS official model/list exposes priority and config/read retains Fast scope");
    server.stop()
}

#[test]
fn fast_catalog_is_transactional_and_official_settings_are_restored() {
    let mut f = Fixture::new();
    let home = f.request.home.clone();
    let original = fs::read_to_string(home.join("config.toml")).unwrap();
    let original = format!("service_tier = 'flex'\nmodel_catalog_json = 'original-models.json'\n{original}");
    fs::write(home.join("config.toml"), &original).unwrap();
    fs::write(home.join("original-models.json"), serde_json::to_vec(&json!({"models":[{
        "slug":"api-a-model", "context_window":123456, "input_modalities":["text","image"],
        "supported_reasoning_levels":[{"effort":"high","description":"High"}],
        "default_service_tier":"priority"
    }]})).unwrap()).unwrap();
    f.select("a", "api-a-model");
    f.request.provider.as_mut().unwrap().native_fast = true;
    inspect(&f.request).unwrap();
    let catalog = f.request.store_root.join("fast-models.json");
    assert!(!catalog.exists());
    f.run().unwrap();
    assert_eq!(inspect(&f.request).unwrap()["nativeFastEnabled"], true);
    let (_, config) = read_config(&home).unwrap();
    assert_eq!(config["model_catalog_json"].as_str(), catalog.to_str());
    assert!(config.get("service_tier").is_none());
    let data: Value = serde_json::from_slice(&fs::read(&catalog).unwrap()).unwrap();
    let model = &data["models"][0];
    assert_eq!(model["service_tiers"][0]["id"], "priority");
    assert_eq!(model["additional_speed_tiers"], json!(["fast"]));
    assert!(model["default_service_tier"].is_null());
    assert_eq!(model["context_window"], 123456);
    assert_eq!(model["input_modalities"], json!(["text", "image"]));
    let operation = journal::list(&f.request.store_root.join("operations"), false).unwrap().remove(0);
    let manifest: journal::Manifest = serde_json::from_slice(&fs::read(f.request.store_root.join("operations").join(operation.id).join("manifest.json")).unwrap()).unwrap();
    assert!(manifest.files.iter().any(|file| file.path == catalog));

    f.select("b", "api-b-model");
    f.run().unwrap();
    assert_eq!(inspect(&f.request).unwrap()["nativeFastEnabled"], false);
    assert!(read_config(&home).unwrap().1.get("model_catalog_json").is_none());
    f.select("official", "official-model");
    f.run().unwrap();
    let (_, config) = read_config(&home).unwrap();
    assert_eq!(config["service_tier"].as_str(), Some("flex"));
    assert_eq!(config["model_catalog_json"].as_str(), Some("original-models.json"));
}

#[test]
fn invalid_fast_catalog_does_not_commit_configuration_or_history() {
    let mut f = Fixture::new();
    fs::write(f.request.home.join("models_cache.json"), "invalid json").unwrap();
    let config = fs::read(f.request.home.join("config.toml")).unwrap();
    let history = fs::read(&f.rollout).unwrap();
    f.select("a", "api-a-model");
    f.request.provider.as_mut().unwrap().native_fast = true;
    assert!(f.run().is_err());
    assert_eq!(fs::read(f.request.home.join("config.toml")).unwrap(), config);
    assert_eq!(fs::read(&f.rollout).unwrap(), history);
}
