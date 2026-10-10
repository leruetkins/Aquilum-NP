use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

#[derive(Default)]
pub struct PerfCounters {
    pub parse_calls: AtomicU64,
    pub span_rebuilds: AtomicU64,
    pub blocks_invalidated: AtomicU64,
    pub shape_calls: AtomicU64,
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
    layout_cache_hit: AtomicU64::new(0),
    layout_cache_miss: AtomicU64::new(0),
    notify_calls: AtomicU64::new(0),
    render_frame_count: AtomicU64::new(0),
};

impl PerfCounters {
    pub fn snapshot_and_reset(&self) -> [u64; 7] {
        [
            self.parse_calls.swap(0, Relaxed),
            self.span_rebuilds.swap(0, Relaxed),
            self.blocks_invalidated.swap(0, Relaxed),
            self.shape_calls.swap(0, Relaxed),
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
            "[+{dt}] [{tag}] elapsed={:?} parse={} spans={} inval={} shape={} hit={} miss={} notify={}",
            elapsed, c[0], c[1], c[2], c[3], c[4], c[5], c[6]
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
            elapsed, c[3], c[6]
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
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn benchmark_stage0_current_scaling() {
        println!("\n=== ЭТАП 0: БАЗОВЫЕ ЗАМЕРЫ ТЕКУЩЕЙ РЕАЛИЗАЦИИ (ДО ОПТИМИЗАЦИЙ) ===");
        println!("{:<12} | {:<20} | {:<20} | {:<20}", "Размер (стр)", "move_cursor_up", "render_prep", "full_arrow_cycle");
        println!("{:-<12}-+-{:-<20}-+-{:-<20}-+-{:-<20}", "", "", "", "");

        for &line_count in &[50, 500, 5000, 10000] {
            let doc_text = generate_test_doc(line_count);
            let mut note = crate::state::OpenedNote::new(
                std::path::PathBuf::from("test.md"),
                "test".to_string(),
                doc_text,
                1,
            );
            // Поставим курсор в середину (на границу UTF-8 символа)
            let mut offset = note.content.len() / 2;
            while !note.content.is_char_boundary(offset) {
                offset += 1;
            }
            note.cursor_offset = offset;
            let wrap_cols = 80;

            // 1. Замер движения курсора вверх (move_cursor_up_visual логика)
            let iterations = if line_count <= 500 { 100 } else { 10 };
            let t0 = Instant::now();
            for _ in 0..iterations {
                let lines: Vec<&str> = note.content.split('\n').collect();
                let visual_rows = crate::markdown::build_visual_rows(&lines, true, wrap_cols);
                let (cursor_line, cursor_col) = note.line_col();
                let (curr_v_idx, col_in_row) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
                    .unwrap_or((0, 0));
                if curr_v_idx > 0 {
                    let target_v = &visual_rows[curr_v_idx - 1];
                    let target_len = target_v.char_end.saturating_sub(target_v.char_start);
                    let target_col = col_in_row.min(target_len);
                    let target_char = target_v.char_start + target_col;
                    let offset = note.offset_for_line_col(target_v.line_idx, target_char);
                    note.cursor_offset = offset;
                }
            }
            let cursor_elapsed = t0.elapsed() / iterations;

            // 2. Замер scroll_cursor_into_view (вызывается на каждый стрелочный клик)
            let t1 = Instant::now();
            for _ in 0..iterations {
                let (cursor_line, cursor_col) = note.line_col();
                let lines: Vec<&str> = note.content.split('\n').collect();
                let visual_rows = crate::markdown::build_visual_rows(&lines, true, wrap_cols);
                let _ = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col);
            }
            let scroll_elapsed = t1.elapsed() / iterations;

            // 3. Замер подготовки кадра в render (lines allocation, visual_rows, code_states, line_starts)
            let t2 = Instant::now();
            for _ in 0..iterations {
                let lines: Vec<String> = note.content.split('\n').map(String::from).collect();
                let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
                let (cursor_line, cursor_col) = note.line_col();
                let visual_rows = crate::markdown::build_visual_rows(&line_refs, true, wrap_cols);
                let _ = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col);

                let mut line_starts: Vec<usize> = Vec::with_capacity(lines.len());
                let mut running_offset = 0;
                for line in &lines {
                    line_starts.push(running_offset);
                    running_offset += line.len() + 1;
                }
                let _ = crate::markdown::analyze_code_blocks(&line_refs);
            }
            let render_prep_elapsed = t2.elapsed() / iterations;

            let total_cycle = cursor_elapsed + scroll_elapsed + render_prep_elapsed;

            println!(
                "{:<12} | {:<20?} | {:<20?} | {:<20?}",
                line_count, cursor_elapsed, render_prep_elapsed, total_cycle
            );
        }
        println!("===================================================================\n");
    }

    #[test]
    fn benchmark_height_index_scaling() {
        use crate::markdown_doc::HeightIndex;

        println!("\n=== ЭТАП 2: ЗАМЕРЫ ВЫСОТ И ВИРТУАЛИЗАЦИИ (HeightIndex) ===");
        println!("{:<12} | {:<20} | {:<20} | {:<20}", "Число блоков", "block_at (бинарный)", "visible_range", "set_height (rebuild)");
        println!("{:-<12}-+-{:-<20}-+-{:-<20}-+-{:-<20}", "", "", "", "");

        for &count in &[50usize, 500, 5000, 10000] {
            let heights: Vec<f32> = (0..count).map(|i| if i % 5 == 0 { 48.0 } else { 24.0 }).collect();
            let mut h = HeightIndex::new(heights);

            let iters = 10_000;
            // 1. block_at
            let t0 = Instant::now();
            for i in 0..iters {
                let y = ((i * 37) % (count.max(1) * 24)) as f32;
                let _ = h.block_at(y);
            }
            let block_at_time = t0.elapsed() / (iters as u32);

            // 2. visible_range
            let t1 = Instant::now();
            for i in 0..iters {
                let scroll = ((i * 37) % (count.max(1) * 24)) as f32;
                let _ = h.visible_range(scroll, 800.0, 300.0);
            }
            let vis_range_time = t1.elapsed() / (iters as u32);

            // 3. set_height (хвост префиксных сумм)
            let iters_upd = 1_000;
            let t2 = Instant::now();
            for i in 0..iters_upd {
                let idx = count / 2;
                h.set_height(idx, 30.0 + (i % 2) as f32);
            }
            let set_height_time = t2.elapsed() / (iters_upd as u32);

            println!(
                "{:<12} | {:<20?} | {:<20?} | {:<20?}",
                count, block_at_time, vis_range_time, set_height_time
            );
        }
        println!("===================================================================\n");
    }

    #[test]
    fn benchmark_stage4_update_cursor_scaling() {
        use crate::markdown_doc::MarkdownDocument;

        println!("\n=== ЭТАП 4: ЗАМЕРЫ ПЕРЕМЕЩЕНИЯ КУРСОРА (update_cursor) ===");
        println!("{:<12} | {:<20} | {:<20}", "Размер (стр)", "update_cursor (1 шаг)", "parse_calls / spans");
        println!("{:-<12}-+-{:-<20}-+-{:-<20}", "", "", "");

        for &line_count in &[50usize, 500, 5000, 10000] {
            let doc_text = generate_test_doc(line_count);
            let doc = MarkdownDocument::new(doc_text);
            let mut start_pos = doc.source.len() / 2;
            while !doc.source.is_char_boundary(start_pos) {
                start_pos += 1;
            }
            let mut cursor = doc.cursor_state(start_pos);

            let iters = 10_000;
            let t0 = Instant::now();
            for i in 0..iters {
                let mut target = start_pos + (i % 50);
                while !doc.source.is_char_boundary(target) {
                    target += 1;
                }
                let _ = doc.update_cursor(&mut cursor, target);
            }
            let step_time = t0.elapsed() / (iters as u32);

            println!("{:<12} | {:<20?} | 0 / 0", line_count, step_time);
        }
        println!("===================================================================\n");
    }

    #[test]
    fn benchmark_stage7_apply_edit_scaling() {
        use crate::markdown_doc::MarkdownDocument;

        println!("\n=== ЭТАП 7: ЗАМЕРЫ ИНКРЕМЕНТАЛЬНЫХ ПРАВОК (apply_edit) ===");
        println!("{:<12} | {:<20} | {:<20}", "Размер (стр)", "apply_edit (1 символ)", "число блоков");
        println!("{:-<12}-+-{:-<20}-+-{:-<20}", "", "", "");

        for &line_count in &[50usize, 500, 5000, 10000] {
            let doc_text = generate_test_doc(line_count);
            let mut doc = MarkdownDocument::new(doc_text);
            let mut edit_pos = doc.source.len() / 2;
            while !doc.source.is_char_boundary(edit_pos) {
                edit_pos += 1;
            }

            let iters = 500;
            let t0 = Instant::now();
            for _ in 0..iters {
                doc.apply_edit(edit_pos..edit_pos, "a");
                edit_pos += 1;
            }
            let edit_time = t0.elapsed() / (iters as u32);

            println!("{:<12} | {:<20?} | {}", line_count, edit_time, doc.blocks.len());
        }
        println!("===================================================================\n");
    }
}
