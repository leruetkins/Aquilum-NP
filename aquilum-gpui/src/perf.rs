use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

#[derive(Default)]
pub struct PerfCounters {
    pub parse_calls: AtomicU64,
    pub span_rebuilds: AtomicU64,
    pub blocks_invalidated: AtomicU64,
    pub shape_calls: AtomicU64,
    pub shape_cache_hits: AtomicU64,
    pub layout_cache_hit: AtomicU64,
    pub layout_cache_miss: AtomicU64,
    pub notify_calls: AtomicU64,
    pub render_frame_count: AtomicU64,
}

pub static PERF: PerfCounters = PerfCounters {
    parse_calls: AtomicU64::new(0),
    span_rebuilds: AtomicU64::new(0),
    blocks_invalidated: AtomicU64::new(0),
    shape_calls: AtomicU64::new(0),
    shape_cache_hits: AtomicU64::new(0),
    layout_cache_hit: AtomicU64::new(0),
    layout_cache_miss: AtomicU64::new(0),
    notify_calls: AtomicU64::new(0),
    render_frame_count: AtomicU64::new(0),
};

impl PerfCounters {
    pub fn snapshot_and_reset(&self) -> [u64; 8] {
        [
            self.parse_calls.swap(0, Relaxed),
            self.span_rebuilds.swap(0, Relaxed),
            self.blocks_invalidated.swap(0, Relaxed),
            self.shape_calls.swap(0, Relaxed),
            self.shape_cache_hits.swap(0, Relaxed),
            self.layout_cache_hit.swap(0, Relaxed),
            self.layout_cache_miss.swap(0, Relaxed),
            self.notify_calls.swap(0, Relaxed),
        ]
    }

    pub fn reset(&self) {
        let _ = self.snapshot_and_reset();
    }

    pub fn log_snapshot(&self, tag: &str, elapsed: std::time::Duration) {
        if !*PERF_LOG_ENABLED { return; }
        let dt = delta_time_str();
        let c = self.snapshot_and_reset();
        eprintln!(
            "[+{dt}] [{tag}] elapsed={:?} parse={} spans={} inval={} shape={} shape_hits={} lay_hit={} lay_miss={} notify={}",
            elapsed, c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]
        );
    }

    pub fn log_key_navigation(
        &self,
        key: &str,
        elapsed: std::time::Duration,
        v_row: usize,
        v_col: usize,
        line: usize,
        col: usize,
        offset: usize,
    ) {
        if !*PERF_LOG_ENABLED { return; }
        let dt = delta_time_str();
        let c = self.snapshot_and_reset();
        eprintln!(
            "[+{dt}] [caret_key:{key}] caret_visual=(row:{v_row}, col:{v_col}) doc=(line:{line}, col:{col}, off:{offset}) elapsed={:?} shape={} notify={}",
            elapsed, c[3], c[7]
        );
    }

    pub fn log_render_frame(
        &self,
        active_v_row: usize,
        active_v_col: usize,
        caret_x: f32,
        visible_start: usize,
        visible_end: usize,
        total_rows: usize,
        elapsed: std::time::Duration,
    ) {
        if !*PERF_LOG_ENABLED { return; }
        let dt = delta_time_str();
        let frame_id = self.render_frame_count.fetch_add(1, Relaxed) + 1;
        let is_visible = active_v_row >= visible_start && active_v_row < visible_end;
        eprintln!(
            "[+{dt}] [caret_render #{frame_id}] caret_visual=(row:{active_v_row}, col:{active_v_col}, x:{caret_x:.1}px) visible_rows={visible_start}..{visible_end}/{total_rows} in_view={is_visible} frame_time={:?}",
            elapsed
        );
    }
}

pub static PERF_LOG_ENABLED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
    cfg!(debug_assertions) || std::env::var_os("VIBECODIUM_PERF").is_some()
});

pub static LAST_LOG_INSTANT: std::sync::OnceLock<std::sync::Mutex<std::time::Instant>> = std::sync::OnceLock::new();

fn delta_time_str() -> String {
    let mutex = LAST_LOG_INSTANT.get_or_init(|| std::sync::Mutex::new(std::time::Instant::now()));
    let mut guard = mutex.lock().unwrap();
    let now = std::time::Instant::now();
    let elapsed = now.duration_since(*guard);
    *guard = now;
    if elapsed.as_millis() >= 1000 {
        format!("{:.2}s", elapsed.as_secs_f32())
    } else {
        format!("{:.1}ms", elapsed.as_secs_f32() * 1000.0)
    }
}

pub fn generate_test_doc(lines: usize) -> String {
    let mut s = String::new();
    for i in 0..lines {
        match i % 5 {
            0 => s.push_str(&format!("# Заголовок {i}\n\n")),
            1 => s.push_str("Текст с **жирным**, *курсивом* и [ссылкой](https://example.com).\n\n"),
            2 => s.push_str("- пункт списка\n- ещё пункт\n\n"),
            3 => s.push_str("```rust\nfn main() {}\n```\n\n"),
            _ => s.push_str("Обычный абзац без разметки.\n\n"),
        }
    }
    s
}

#[cfg(test)]
mod tests;
