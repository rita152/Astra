//! Explicitly invoked scroll/layout benchmark; timing is not a screen FPS metric.

use super::*;
use gpui::{Bounds, TestApp, WindowBounds, WindowOptions, point, size};

#[test]
#[ignore = "manual long-diff scroll benchmark"]
fn long_diff_scroll_timings() {
    let path = std::env::var("GPUI_DIFF_BENCH_PATCH").expect("set GPUI_DIFF_BENCH_PATCH");
    let patch = std::fs::read_to_string(path).unwrap();
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.), px(0.)),
                size(px(710.), px(923.)),
            ))),
            ..Default::default()
        },
        |_, cx| {
            let mut panel = ReviewPanel::new(std::env::temp_dir(), ThemeMode::Dark, cx);
            panel.active = false;
            panel.generation += 1;
            panel.loading = true;
            panel.tree_open = false;
            panel.snapshot = Arc::new(Snapshot {
                root: std::env::temp_dir(),
                files: git_review::parse_unified(&patch),
                ..Default::default()
            });
            panel.rebuild(cx);
            panel
        },
    );
    window.draw();
    window.draw();
    let mut results = Vec::new();
    for (name, words, split, deep) in [
        ("unified_shallow", false, false, false),
        ("unified_deep", false, false, true),
        ("word_diff_deep", true, false, true),
        ("split_word_diff_deep", true, true, true),
    ] {
        window.update(|p, _, cx| {
            p.words = words;
            p.split = split;
            p.rebuild(cx);
            p.scroll.scroll_to(ListOffset {
                item_ix: if deep {
                    p.rows.len().saturating_sub(1200)
                } else {
                    200
                },
                offset_in_item: px(0.),
            });
        });
        window.draw();
        window.draw();
        let mut samples = Vec::new();
        for frame in 0..100 {
            let start = std::time::Instant::now();
            window.simulate_scroll(
                point(px(300.), px(450.)),
                point(px(0.), px(if frame < 50 { -60. } else { 60. })),
            );
            window.draw();
            samples.push(start.elapsed().as_secs_f64() * 1000.);
        }
        samples.sort_by(f64::total_cmp);
        let result = serde_json::json!({
            "scenario": name, "samples": samples.len(),
            "median_ms": samples[50], "p95_ms": samples[94], "max_ms": samples[99],
        });
        println!("{result}");
        results.push(result);
    }
    if let Ok(output) = std::env::var("GPUI_DIFF_BENCH_OUTPUT") {
        std::fs::write(output, serde_json::to_vec_pretty(&results).unwrap()).unwrap();
    }
}
