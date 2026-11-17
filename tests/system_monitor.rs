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
