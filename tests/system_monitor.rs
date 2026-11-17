use island::system_monitor::{counter_rate, cpu_percent, gpu_percent, History, Sample, Settings};

#[test]
fn cpu_uses_kernel_including_idle_and_rejects_reset_counters() {
    assert_eq!(cpu_percent([100, 200, 100], [120, 260, 140]), Some(80.));
    assert_eq!(cpu_percent([100, 200, 100], [100, 200, 100]), None);
    assert_eq!(cpu_percent([100, 200, 100], [0, 0, 0]), None);
    assert_eq!(cpu_percent([0, 0, 0], [200, 100, 0]), None);
}

#[test]
fn traffic_rates_respect_elapsed_time_and_adapter_resets() {
    assert_eq!(counter_rate(100, 500, 2.), Some(200.));
    assert_eq!(counter_rate(500, 100, 2.), None);
    assert_eq!(counter_rate(100, 500, 0.), None);
    assert_eq!(counter_rate(100, 500, f64::NAN), None);
}

#[test]
fn traffic_rate_switches_units_before_a_fourth_integer_digit_is_displayed() {
    use island::system_monitor::rate;
    let kb = 1024.;
    let mb = kb * 1024.;
    let gb = mb * 1024.;
    for (value, expected) in [
        (0., "0 KB/s"),
        (9. * kb, "9 KB/s"),
        (999.49 * kb, "999 KB/s"),
        (999.5 * kb, "1.0 MB/s"),
        (99.94 * mb, "99.9 MB/s"),
        (99.96 * mb, "100 MB/s"),
        (999.49 * mb, "999 MB/s"),
        (999.5 * mb, "1.0 GB/s"),
        (999.5 * gb, "1.0 TB/s"),
    ] {
        assert_eq!(rate(Some(value)), expected, "原始速率：{value}");
    }
    for value in [None, Some(f64::NAN), Some(f64::INFINITY), Some(-1.)] {
        assert_eq!(rate(value), "—", "无效读数不应显示为真实速度");
    }
}

#[test]
fn gpu_sums_processes_per_engine_then_takes_busiest_engine() {
    let rows = [
        ("pid_1_luid_a_phys_0_eng_0_engtype_3D", 30.),
        ("pid_2_luid_a_phys_0_eng_0_engtype_3D", 45.),
        ("pid_1_luid_a_phys_0_eng_1_engtype_Copy", 60.),
    ];
    assert_eq!(gpu_percent(rows), Some(75.));
    assert_eq!(gpu_percent(std::iter::empty()), None);
    assert_eq!(
        gpu_percent([("pid_1_luid_a_phys_0_eng_0_engtype_3D", f64::NAN)]),
        None
    );
}

#[test]
fn history_is_bounded_and_keeps_unavailable_samples_as_gaps() {
    let mut history = History::default();
    for _ in 0..100 {
        history.push(&Sample::default());
    }
    assert_eq!(history.cpu.len(), 60);
    assert!(history.cpu.iter().all(Option::is_none));
}

#[test]
fn monitor_options_normalize_invalid_intervals_and_colors() {
    let mut settings:Settings=serde_json::from_str(r##"{"interval":0,"custom_colors":["bad","#22BBCC","#112233","#123456","#654321","#FFFFFF"]}"##).unwrap();
    settings.normalize();
    assert_eq!(settings.interval, 1);
    assert_eq!(settings.custom_colors[1], "#22BBCC");
    assert!(settings
        .custom_colors
        .iter()
        .all(|c| island::transfer::color(c).is_some()));
}

#[test]
fn monitor_metric_switches_migrate_and_round_trip_independently() {
    let legacy: Settings = serde_json::from_str(r#"{"show_storage":false}"#).unwrap();
    let legacy_json = serde_json::to_value(&legacy).unwrap();
    for name in [
        "show_cpu",
        "show_memory",
        "show_gpu",
        "show_download",
        "show_upload",
    ] {
        assert_eq!(legacy_json[name], true, "旧配置应保留展开读数：{name}");
    }
    assert_eq!(legacy_json["show_storage"], false);
    assert_eq!(
        legacy_json["capsule_readings"],
        serde_json::json!([true, true, false, false, false, false])
    );

    let chosen = serde_json::json!({"show_cpu":false,"show_memory":false,"show_gpu":true,
        "show_download":false,"show_upload":true,"show_storage":false,"show_processes":false,
        "capsule_readings":[false,false,true,true,false,true]});
    let restored: Settings = serde_json::from_value(chosen.clone()).unwrap();
    let round_trip = serde_json::to_value(&restored).unwrap();
    for (key, value) in chosen.as_object().unwrap() {
        assert_eq!(&round_trip[key], value, "开关应独立保存：{key}");
    }
}

#[test]
fn monitor_sampling_requests_only_readings_in_visible_views() {
    use island::system_monitor::Reading;
    let mut settings = Settings::default();
    settings.show_cpu = false;
    settings.show_memory = false;
    settings.show_storage = false;
    settings.capsule_readings = [true, false, false, false, true, true];
    for expanded in [false, true] {
        for capsule in [false, true] {
            let sampling = settings.sampling_for_views(expanded, capsule);
            for reading in Reading::ALL {
                assert_eq!(
                    sampling.expanded_reading(reading),
                    (expanded && settings.expanded_reading(reading))
                        || (capsule && settings.capsule_readings[reading as usize])
                );
            }
            assert_eq!(sampling.show_processes, expanded);
            assert_eq!(sampling.has_readings(), expanded || capsule);
        }
    }
    assert!(
        !settings.sampling_for_views(false, true).show_gpu,
        "单独使用网速和磁盘胶囊时不能申请 GPU"
    );
}

#[test]
fn monitor_storage_capsule_uses_weighted_total_and_preserves_unavailable_values() {
    use island::system_monitor::{Drive, Reading};
    let mut sample = Sample::default();
    assert_eq!(Reading::Storage.value(&sample), "—");
    sample.drives = vec![
        Drive {
            name: "C:".into(),
            used: 50,
            total: 100,
        },
        Drive {
            name: "D:".into(),
            used: 0,
            total: 300,
        },
    ];
    assert_eq!(sample.storage_percent(), Some(12.5));
    sample.drives[1].total = 0;
    assert_eq!(sample.storage_percent(), Some(50.));
}

#[test]
#[cfg(windows)]
#[ignore = "只读验证本机 Windows 性能采样，不输出进程名称"]
fn native_monitor_reads_live_cpu_memory_network_and_storage() {
    let mut sampler = island::system_monitor::native::Sampler::default();
    let settings = Settings::default();
    let first = sampler.sample(&settings);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let second = sampler.sample(&settings);
    assert!(first.memory_total.is_some_and(|v| v > 0));
    assert!(second.cpu.is_some_and(|v| (0.0..=100.).contains(&v)));
    assert!(second.download.is_some_and(|v| v >= 0.));
    assert!(second.drives.iter().all(|d| d.used <= d.total));
    println!(
        "Windows 实时采样通过：CPU、内存、网络、磁盘；GPU 可用：{}",
        second.gpu.is_some()
    );
}
