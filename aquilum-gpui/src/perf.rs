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

/// Simulates holding a key: `count` characters typed one by one through the
/// full live pipeline a keystroke goes through — `insert_text` (rope mutation
/// + undo step) and the per-keystroke view bookkeeping.
///
/// Returns (total time, per-character mean). This is the "characters per
/// second" number to compare against a keyboard's key-repeat rate (~30 cps):
/// the mean must stay far below 33 ms/char for repeat to feel instant.
#[cfg(test)]
fn measure_key_repeat(doc: &mut crate::document_buffer::DocumentBuffer, count: usize, at_eof: bool) -> (std::time::Duration, std::time::Duration) {
    let mut view = crate::workspace::EditorViewState::new(crate::workspace::TabId::new(), doc.id);
    let mut anchor = if at_eof { doc.len() } else { doc.len() / 2 };
    while anchor > 0 && !doc.is_char_boundary(anchor) {
        anchor -= 1;
    }
    view.cursor_offset = anchor;
    let total_lines = doc.line_index.lines();
    view.ensure_visual_cache(doc, true, 80, if at_eof { total_lines.saturating_sub(1) } else { total_lines / 2 });

    let t0 = std::time::Instant::now();
    for _ in 0..count {
        view.insert_text(doc, "a");
    }
    let total = t0.elapsed();
    let per_char = total / (count as u32);
    (total, per_char)
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
            let doc_id = crate::document_buffer::DocumentId::new();
            let tab_id = crate::workspace::TabId::new();
            let doc = crate::document_buffer::DocumentBuffer::new(
                doc_id,
                std::path::PathBuf::from("test.md"),
                "test".to_string(),
                doc_text,
                1,
            );
            let mut view = crate::workspace::EditorViewState::new(tab_id, doc_id);
            // Поставим курсор в середину (на границу UTF-8 символа)
            let mut offset = doc.len() / 2;
            while !doc.is_char_boundary(offset) {
                offset += 1;
            }
            view.cursor_offset = offset;
            let wrap_cols = 80;

            // 1. Замер движения курсора вверх (move_cursor_up_visual логика)
            let iterations = if line_count <= 500 { 100 } else { 10 };
            let full_text = doc.content();
            let t0 = Instant::now();
            for _ in 0..iterations {
                let lines: Vec<&str> = full_text.split('\n').collect();
                let visual_rows = crate::markdown::build_visual_rows(&lines, true, wrap_cols);
                let (cursor_line, cursor_col) = view.line_col(&doc);
                let (curr_v_idx, col_in_row) = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col)
                    .unwrap_or((0, 0));
                if curr_v_idx > 0 {
                    let target_v = &visual_rows[curr_v_idx - 1];
                    let target_len = target_v.char_end.saturating_sub(target_v.char_start);
                    let target_col = col_in_row.min(target_len);
                    let target_char = target_v.char_start + target_col;
                    let offset = view.offset_for_line_col(&doc, target_v.line_idx, target_char);
                    view.cursor_offset = offset;
                }
            }
            let cursor_elapsed = t0.elapsed() / iterations;

            // 2. Замер scroll_cursor_into_view (вызывается на каждый стрелочный клик)
            let t1 = Instant::now();
            for _ in 0..iterations {
                let (cursor_line, cursor_col) = view.line_col(&doc);
                let lines: Vec<&str> = full_text.split('\n').collect();
                let visual_rows = crate::markdown::build_visual_rows(&lines, true, wrap_cols);
                let _ = crate::markdown::find_visual_row_for_cursor(&visual_rows, cursor_line, cursor_col);
            }
            let scroll_elapsed = t1.elapsed() / iterations;

            // 3. Замер подготовки кадра в render (lines allocation, visual_rows, code_states, line_starts)
            let t2 = Instant::now();
            for _ in 0..iterations {
                let lines: Vec<String> = doc.content().split('\n').map(String::from).collect();
                let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
                let (cursor_line, cursor_col) = view.line_col(&doc);
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

    /// "Hold a key" benchmark: characters per second through the full live
    /// pipeline. Key-repeat is ~30 chars/s; the per-char mean must stay
    /// well below 33 ms for repeat to feel instant.
    #[test]
    fn benchmark_key_repeat_throughput() {
        println!("\n=== ЗАМЕР УДЕРЖАНИЯ КЛАВИШИ: СИМВОЛОВ В СЕКУНДУ (полный пайплайн ввода) ===");
        println!("{:<10} | {:<10} | {:<16} | {:<16} | {:<12}", "Строк", "Позиция", "время (N симв)", "мс/символ", "симв/с");
        println!("{:-<10}-+-{:-<10}-+-{:-<16}-+-{:-<16}-+-{:-<12}", "", "", "", "", "");

        const COUNT: usize = 300;
        const BUDGET_MS: u128 = 10;

        for &line_count in &[50usize, 5000, 50_000] {
            for at_eof in [false, true] {
                let doc_text = generate_test_doc(line_count);
                let mut doc = crate::document_buffer::DocumentBuffer::new(
                    crate::document_buffer::DocumentId::new(),
                    std::path::PathBuf::from("test.md"),
                    "test".to_string(),
                    doc_text,
                    1,
                );
                let (total, per_char) = measure_key_repeat(&mut doc, COUNT, at_eof);
                let cps = COUNT as f64 / total.as_secs_f64();
                let position = if at_eof { "конец" } else { "середина" };
                println!("{:<10} | {:<10} | {:<16?} | {:<16.3} | {:<12.0}", line_count, position, total, per_char.as_secs_f64() * 1000.0, cps);
                assert!(
                    per_char.as_millis() < BUDGET_MS,
                    "ввод {} на {} строках: {:.3} мс/символ (лимит {BUDGET_MS} мс) — удержание будет подлагивать",
                    position,
                    line_count,
                    per_char.as_secs_f64() * 1000.0
                );
            }
        }
        println!("===================================================================\n");
    }

    #[test]
    fn benchmark_rope_typing_at_top() {
        println!("\n=== ЗАМЕРЫ СКОРОСТИ НАБОРА СВЕРХУ И СНИЗУ ФАЙЛА (DocumentBuffer) ===");
        println!("{:<12} | {:<12} | {:<20} | {:<20}", "Строк", "Размер (МБ)", "Ввод сверху (0)", "Ввод снизу (EOF)");
        println!("{:-<12}-+-{:-<12}-+-{:-<20}-+-{:-<20}", "", "", "", "");

        for &line_count in &[5000, 20_000, 50_000] {
            let doc_text = generate_test_doc(line_count);
            let mb = doc_text.len() as f64 / (1024.0 * 1024.0);

            // 1. Ввод сверху (offset 0)
            let mut doc_top = crate::document_buffer::DocumentBuffer::new(
                crate::document_buffer::DocumentId::new(),
                std::path::PathBuf::from("test.md"),
                "test".to_string(),
                doc_text.clone(),
                1,
            );
            let mut view_top = crate::workspace::EditorViewState::new(
                crate::workspace::TabId::new(),
                doc_top.id,
            );
            view_top.cursor_offset = 0;
            view_top.ensure_visual_cache(&mut doc_top, true, 80, 0);

            let iters = 500;
            let t_top = Instant::now();
            for _ in 0..iters {
                view_top.insert_text(&mut doc_top, "a");
            }
            let top_time = t_top.elapsed() / (iters as u32);

            // 2. Ввод снизу (EOF)
            let mut doc_bot = crate::document_buffer::DocumentBuffer::new(
                crate::document_buffer::DocumentId::new(),
                std::path::PathBuf::from("test.md"),
                "test".to_string(),
                doc_text,
                1,
            );
            let mut view_bot = crate::workspace::EditorViewState::new(
                crate::workspace::TabId::new(),
                doc_bot.id,
            );
            let eof = doc_bot.len();
            view_bot.cursor_offset = eof;
            let total_lines = doc_bot.line_index.lines();
            view_bot.ensure_visual_cache(&mut doc_bot, true, 80, total_lines);

            let t_bot = Instant::now();
            for _ in 0..iters {
                view_bot.insert_text(&mut doc_bot, "a");
            }
            let bot_time = t_bot.elapsed() / (iters as u32);

            println!("{:<12} | {:<12.1} | {:<20?} | {:<20?}", line_count, mb, top_time, bot_time);
            assert!(top_time.as_micros() < 500, "Ввод сверху слишком медленный: {:?}", top_time);
            assert!(bot_time.as_micros() < 500, "Ввод снизу слишком медленный: {:?}", bot_time);
        }
        println!("===================================================================\n");
    }

    #[test]
    fn benchmark_create_note_and_tab_switch_with_large_file() {
        println!("\n=== ЗАМЕРЫ СОЗДАНИЯ ЗАМЕТКИ И ПЕРЕКЛЮЧЕНИЯ ВКЛАДОК С БОЛЬШИМ ФАЙЛОМ ===");
        let temp_dir = std::env::temp_dir().join(format!("aquilum_perf_tab_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let large_path = temp_dir.join("Large.md");
        let large_text = generate_test_doc(30_000);
        let large_mb = large_text.len() as f64 / (1024.0 * 1024.0);
        std::fs::write(&large_path, &large_text).unwrap();

        let mut state = crate::state::AppState::new();
        state.workspace_path = Some(temp_dir.clone());
        state.vault = Some(crate::vault::VaultCore::open(temp_dir.clone()));

        // 1. Открытие большого файла
        let t_open = Instant::now();
        state.open_note(large_path.clone());
        let open_time = t_open.elapsed();

        // 2. Создание новой заметки при открытом большом файле
        let t_create = Instant::now();
        let new_note = state.create_new_note().expect("Должна создаться новая заметка");
        let create_time = t_create.elapsed();

        // 3. Переключение обратно на большой файл
        let t_switch_large = Instant::now();
        state.open_note(large_path.clone());
        let switch_to_large_time = t_switch_large.elapsed();

        // 4. Переключение обратно на новую заметку
        let t_switch_small = Instant::now();
        state.open_note(new_note);
        let switch_to_small_time = t_switch_small.elapsed();

        println!("Размер большого файла: {:.1} МБ", large_mb);
        println!("1. Открытие большого файла:             {:?}", open_time);
        println!("2. Создание новой заметки:               {:?}", create_time);
        println!("3. Переключение на вкладку с большим:   {:?}", switch_to_large_time);
        println!("4. Переключение на вкладку с маленьким: {:?}", switch_to_small_time);
        println!("===================================================================\n");

        assert!(create_time.as_millis() < 100, "Создание заметки должно быть мгновенным (< 100ms): {:?}", create_time);
        assert!(switch_to_large_time.as_millis() < 50, "Переключение на большой файл должно быть мгновенным (< 50ms): {:?}", switch_to_large_time);
        assert!(switch_to_small_time.as_millis() < 50, "Переключение на малый файл должно быть мгновенным (< 50ms): {:?}", switch_to_small_time);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn benchmark_visual_row_virtualization_20k_50k() {
        use crate::markdown::{build_visual_rows, find_visual_row_for_cursor, VisualRowHeightIndex};

        println!("\n=== БЕНЧМАРК ВИРТУАЛИЗАЦИИ РЕДАКТОРА (20 000 и 50 000 СТРОК) ===");
        println!(
            "{:<12} | {:<16} | {:<16} | {:<16} | {:<16}",
            "Строк", "Индекс высот", "visible_range", "v_idx_at_y", "find_cursor"
        );
        println!("{:-<12}-+-{:-<16}-+-{:-<16}-+-{:-<16}-+-{:-<16}", "", "", "", "", "");

        for &count in &[20_000usize, 50_000usize] {
            let lines: Vec<String> = (0..count)
                .map(|i| {
                    if i % 20 == 0 {
                        format!("## Heading 2 for line {}", i)
                    } else if i % 10 == 0 {
                        format!("# Heading 1 for line {}", i)
                    } else {
                        format!("Regular paragraph line content for item {}", i)
                    }
                })
                .collect();
            let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
            let visual_rows = build_visual_rows(&line_refs, false, 0);

            let t0 = Instant::now();
            let index = VisualRowHeightIndex::from_rows(&lines, &visual_rows, None);
            let index_time = t0.elapsed();

            let iters = 10_000;
            let t1 = Instant::now();
            for i in 0..iters {
                let scroll_y = ((i % 1000) as f64) * 50.0;
                let _ = index.visible_range(scroll_y, 800.0, 300.0);
            }
            let range_time = t1.elapsed() / (iters as u32);

            let t2 = Instant::now();
            for i in 0..iters {
                let y = ((i % 1000) as f64) * 60.0;
                let _ = index.v_idx_at_y(y);
            }
            let lookup_time = t2.elapsed() / (iters as u32);

            let t3 = Instant::now();
            for i in 0..iters {
                let cursor_line = (i * 7) % count;
                let _ = find_visual_row_for_cursor(&visual_rows, cursor_line, 10);
            }
            let cursor_time = t3.elapsed() / (iters as u32);

            println!(
                "{:<12} | {:<16?} | {:<16?} | {:<16?} | {:<16?}",
                count, index_time, range_time, lookup_time, cursor_time
            );

            // Assert performance requirements:
            // 1. Range lookup must be < 1 microsecond (1000ns)
            assert!(range_time.as_nanos() < 10_000, "visible_range took too long: {:?}", range_time);
            // 2. Y lookup must be < 1 microsecond
            assert!(lookup_time.as_nanos() < 10_000, "v_idx_at_y took too long: {:?}", lookup_time);
            // 3. Cursor lookup must be < 1 microsecond
            assert!(cursor_time.as_nanos() < 10_000, "find_visual_row_for_cursor took too long: {:?}", cursor_time);
        }
        println!("===================================================================\n");
    }

    fn mb_str(bytes: usize) -> String {
        format!("{:.1}", bytes as f64 / (1024.0 * 1024.0))
    }

    /// Budget of the open path: time and bytes spent just to open a note.
    ///
    /// The point of this benchmark is the *auxiliary* memory column — everything
    /// the editor builds besides the text itself. It is what makes opening a
    /// huge note expensive, and it must not scale with the document.
    ///
    /// Sizes come from `VIBECODIUM_BENCH_LINES` (comma-separated, default
    /// `20000,100000`), e.g. a real measurement:
    /// `VIBECODIUM_BENCH_LINES=4000000 cargo test --bin vibecodium
    /// benchmark_open_large_note -- --nocapture --test-threads=1`
    #[test]
    fn benchmark_open_large_note() {
        let sizes: Vec<usize> = std::env::var("VIBECODIUM_BENCH_LINES")
            .ok()
            .and_then(|v| {
                v.split(',')
                    .map(|part| part.trim().parse::<usize>().ok())
                    .collect::<Option<Vec<usize>>>()
            })
            .unwrap_or_else(|| vec![20_000, 100_000]);

        println!("\n=== ОТКРЫТИЕ ЗАМЕТКИ: ВРЕМЯ И ПАМЯТЬ НА ПУТИ ОТКРЫТИЯ ===");
        println!(
            "{:>10} | {:>7} | {:>10} | {:>10} | {:>9} | {:>9} | {:>9} | {:>9} | {:>9} | {:>9}",
            "строк", "МБ", "open", "cache", "lines", "vrows", "codes", "starts", "heights", "итого"
        );
        println!(
            "{:-<10}-+-{:-<7}-+-{:-<10}-+-{:-<10}-+-{:-<9}-+-{:-<9}-+-{:-<9}-+-{:-<9}-+-{:-<9}-+-{:-<9}",
            "", "", "", "", "", "", "", "", "", ""
        );

        for &lines in &sizes {
            let text = generate_test_doc(lines);
            let doc_mb = text.len() as f64 / (1024.0 * 1024.0);

            let t_open = Instant::now();
            let doc_id = crate::document_buffer::DocumentId::new();
            let tab_id = crate::workspace::TabId::new();
            let mut doc = crate::document_buffer::DocumentBuffer::new(
                doc_id,
                std::path::PathBuf::from("bench.md"),
                "bench".to_string(),
                text,
                1,
            );
            let mut view = crate::workspace::EditorViewState::new(tab_id, doc_id);
            let open = t_open.elapsed();

            let t_cache = Instant::now();
            view.ensure_visual_cache(&mut doc, true, 80, 0);
            let cache = t_cache.elapsed();

            // Auxiliary memory: everything except the text itself. Every column
            // but `starts` measures the *window*, so none of them grows with the
            // document — that is the whole point of the open-path budget, and it
            // is what the `строк` column is there to show.
            let lines_bytes = view.window.lines.bytes();
            let vrows = view.window.visual_rows.capacity()
                * std::mem::size_of::<crate::markdown::VisualRow>();
            let codes = view.window.code_states.bytes();
            // Line addressing used to be a `Vec<usize>` with one entry per line;
            // it is now a checkpoint index, so this column is kilobytes.
            let starts = doc.line_index.bytes();
            let heights = view.window.height_index.bytes();
            let total = lines_bytes + vrows + codes + starts + heights;

            println!(
                "{:>10} | {:>7.1} | {:>10?} | {:>10?} | {:>9} | {:>9} | {:>9} | {:>9} | {:>9} | {:>9}",
                lines,
                doc_mb,
                open,
                cache,
                mb_str(lines_bytes),
                mb_str(vrows),
                mb_str(codes),
                mb_str(starts),
                mb_str(heights),
                mb_str(total)
            );
        }
        println!("===================================================================\n");
    }

    #[test]
    fn benchmark_open_note_with_many_tables() {
        println!("\n=== ОТКРЫТИЕ ЗАМЕТКИ С МНОЖЕСТВОМ ТАБЛИЦ: ЗАМЕРЫ ВРЕМЕНИ ===");
        let mut text = String::new();
        // Generate a document with 2,000 tables (each table is 5 lines = 10,000 lines + paragraphs = ~20,000 lines)
        for i in 0..2000 {
            text.push_str(&format!("# Section {}\n\nParagraph text before table {}.\n\n", i, i));
            text.push_str("| Col A | Col B | Col C | Col D |\n| :--- | :---: | ---: | :--- |\n| Val 1 | Val 2 | Val 3 | Val 4 |\n| Row 2 | Data 2 | More | Extra |\n\n");
        }

        let doc_mb = text.len() as f64 / (1024.0 * 1024.0);
        let t_open = Instant::now();
        let doc_id = crate::document_buffer::DocumentId::new();
        let tab_id = crate::workspace::TabId::new();
        let mut doc = crate::document_buffer::DocumentBuffer::new(
            doc_id,
            std::path::PathBuf::from("bench_many_tables.md"),
            "bench_many_tables".to_string(),
            text,
            1,
        );
        let mut view = crate::workspace::EditorViewState::new(tab_id, doc_id);
        let open_dur = t_open.elapsed();

        let t_cache = Instant::now();
        view.ensure_visual_cache(&mut doc, true, 80, 0);
        let cache_dur = t_cache.elapsed();

        println!("Lines: {}, MB: {:.2}", doc.line_index.lines(), doc_mb);
        println!("Buffer open time: {:?}", open_dur);
        println!("Windowed visual cache & table build time: {:?}", cache_dur);
        println!("Total time to first frame: {:?}", open_dur + cache_dur);
    }
}
