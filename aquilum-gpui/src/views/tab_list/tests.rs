use gpui::SharedString;
use gpui::{px, Pixels};
use crate::icons;
    use super::*;

    fn approx(left: Pixels, right: Pixels) -> bool {
        (left - right).abs() < px(0.01)
    }

    // The tests below use list_top = 100 and list_height = 300, so the dead zone is
    // clamp(300 * 0.10, 28, 56) = 30 px: the proportional band is 130..370.

    #[test]
    fn hover_row_resolves_to_a_single_row() {
        let at = |y: f32| hover_row_at(y, 100.0, 280.0, 0.0, ROW_HEIGHT, 10);
        assert_eq!(at(100.0), Some(0));
        assert_eq!(at(100.0 + 3.0 * ROW_HEIGHT + 1.0), Some(3));
        // Exactly on a seam: floor wins deterministically, never both rows.
        assert_eq!(at(100.0 + 2.0 * ROW_HEIGHT), Some(2));
        // Exactly on the bottom edge there is no row anymore (past the last one).
        assert_eq!(at(100.0 + 280.0), None);
        // ...but a hair above still hits the last row.
        assert_eq!(at(100.0 + 279.9), Some(9));
    }

    #[test]
    fn hover_row_follows_a_scrolled_offset() {
        // Viewport scrolled by 56 px (two rows): content [56, 336).
        let at = |y: f32| hover_row_at(y, 100.0, 280.0, -56.0, ROW_HEIGHT, 10);
        assert_eq!(at(100.0), Some(2));
        // Bottom edge shows content 336: clamps into the list.
        // Bottom edge shows content past the list: nothing hovered.
        assert_eq!(at(100.0 + 280.0), None);
    }

    #[test]
    fn hover_row_is_none_outside_the_list() {
        let at = |y: f32| hover_row_at(y, 100.0, 280.0, 0.0, ROW_HEIGHT, 10);
        assert_eq!(at(99.9), None);
        assert_eq!(at(100.0 + 280.0 + 0.1), None);
        assert_eq!(hover_row_at(150.0, 100.0, 280.0, 0.0, ROW_HEIGHT, 0), None);
        assert_eq!(hover_row_at(150.0, 100.0, 0.0, 0.0, ROW_HEIGHT, 10), None);
    }

    #[test]
    fn hover_header_maps_rows_and_skips_dividers() {
        assert_eq!(hover_header_at(-0.1), None);
        assert_eq!(hover_header_at(0.0), Some(0));
        assert_eq!(hover_header_at(27.9), Some(0));
        assert_eq!(hover_header_at(28.0), Some(1));
        // First divider: no row.
        assert_eq!(hover_header_at(56.0), None);
        assert_eq!(hover_header_at(64.9), None);
        assert_eq!(hover_header_at(65.0), Some(2));
        assert_eq!(hover_header_at(92.9), Some(2));
        // Second divider and below: nothing.
        assert_eq!(hover_header_at(93.0), None);
        assert_eq!(hover_header_at(101.9), None);
        assert_eq!(hover_header_at(HEADER_H), None);
        assert_eq!(hover_header_at(HEADER_H + 10.0), None);
    }

    #[test]
    fn edge_pad_is_a_fraction_of_the_height_within_bounds() {
        assert!(approx(edge_pad(px(300.0)), px(30.0)));
        // 10% of 1000 is 100, capped at EDGE_PAD_MAX.
        assert!(approx(edge_pad(px(1000.0)), px(EDGE_PAD_MAX)));
        // 10% of 150 is 15, raised to one row...
        assert!(approx(edge_pad(px(150.0)), px(ROW_HEIGHT)));
        // ...but never more than a third of a very short list.
        assert!(approx(edge_pad(px(60.0)), px(20.0)));
    }

    #[test]
    fn cursor_at_the_top_of_the_band_shows_the_beginning() {
        let target = cursor_target_offset(px(130.0), px(100.0), px(300.0), px(1000.0));
        assert!(approx(target, px(0.0)));
    }

    #[test]
    fn cursor_at_the_bottom_of_the_band_shows_the_end() {
        let target = cursor_target_offset(px(370.0), px(100.0), px(300.0), px(1000.0));
        assert!(approx(target, px(-1000.0)));
    }

    #[test]
    fn the_last_row_is_fully_visible_one_row_before_the_edge() {
        // One row above the bottom border (400 - 28 = 372) is already the very end.
        let target =
            cursor_target_offset(px(400.0 - ROW_HEIGHT), px(100.0), px(300.0), px(1000.0));
        assert!(approx(target, px(-1000.0)));
    }

    #[test]
    fn the_first_row_is_fully_visible_one_row_after_the_edge() {
        let target =
            cursor_target_offset(px(100.0 + ROW_HEIGHT), px(100.0), px(300.0), px(1000.0));
        assert!(approx(target, px(0.0)));
    }

    #[test]
    fn cursor_in_the_middle_lands_in_the_middle() {
        let target = cursor_target_offset(px(250.0), px(100.0), px(300.0), px(1000.0));
        assert!(approx(target, px(-500.0)));
    }

    #[test]
    fn cursor_outside_the_list_is_clamped() {
        let top = cursor_target_offset(px(0.0), px(100.0), px(300.0), px(1000.0));
        assert!(approx(top, px(0.0)));
        let bottom = cursor_target_offset(px(900.0), px(100.0), px(300.0), px(1000.0));
        assert!(approx(bottom, px(-1000.0)));
    }

    #[test]
    fn a_longer_list_moves_faster_for_the_same_cursor_travel() {
        // Same cursor travel and same list height, but a longer scrollable range:
        // the offset delta grows with max_scroll, which is the whole point of
        // deriving the speed from max_scroll / list_height.
        let from = px(150.0);
        let to = px(200.0);
        let list_top = px(100.0);
        let list_height = px(300.0);

        let few = cursor_target_offset(to, list_top, list_height, px(200.0))
            - cursor_target_offset(from, list_top, list_height, px(200.0));
        let many = cursor_target_offset(to, list_top, list_height, px(2000.0))
            - cursor_target_offset(from, list_top, list_height, px(2000.0));

        assert!(many.abs() > few.abs() * 5.0);
    }

    #[test]
    fn a_visible_row_is_not_moved() {
        // Viewport 0..300, row 200..228 is fully visible.
        let target = nearest_offset(px(200.0), px(28.0), px(0.0), px(300.0), px(0.0), px(1000.0));
        assert!(approx(target, px(0.0)));
    }

    #[test]
    fn a_row_above_the_viewport_scrolls_up_by_the_minimum() {
        // Viewport 500..800, row 200..228.
        let target =
            nearest_offset(px(200.0), px(28.0), px(500.0), px(300.0), px(-500.0), px(1000.0));
        assert!(approx(target, px(-200.0)));
    }

    #[test]
    fn a_row_below_the_viewport_scrolls_down_by_the_minimum() {
        // Viewport 0..300, row 400..428.
        let target = nearest_offset(px(400.0), px(28.0), px(0.0), px(300.0), px(0.0), px(1000.0));
        assert!(approx(target, px(-128.0)));
    }

    #[test]
    fn nearest_is_clamped_to_the_scrollable_range() {
        // The minimum movement would be -128, but only 100 px are scrollable.
        let target = nearest_offset(px(400.0), px(28.0), px(0.0), px(300.0), px(0.0), px(100.0));
        assert!(approx(target, px(-100.0)));
    }

    #[test]
    fn smoothing_alpha_converges_equally_at_any_fps() {
        // One second of motion towards -1000 lands on the target at both rates.
        let run = |frames: u32| {
            let mut pos = 0.0_f32;
            let target = -1000.0;
            for _ in 0..frames {
                pos += (target - pos) * smoothing_alpha(1.0 / frames as f32, 50.0);
            }
            pos
        };
        assert!((run(60) - run(144)).abs() < 1.0);
        assert!((run(60) + 1000.0).abs() < 2.0);
        // Zero tau means "already there".
        assert_eq!(smoothing_alpha(1.0 / 60.0, 0.0), 1.0);
    }

    #[test]
    fn snapping_lands_on_device_pixels() {
        // 150%: a device pixel is 1/1.5 px. 10.3 * 1.5 = 15.45 -> 15 -> 10.0.
        assert!(approx(snap_to_device_pixel(px(10.3), 1.5), px(10.0)));
        assert!(approx(snap_to_device_pixel(px(10.4), 1.0), px(10.0)));
        assert!(approx(snap_to_device_pixel(px(-13.47), 2.0), px(-13.5)));
    }

    #[test]
    fn panel_chrome_matches_padding_and_border() {
        // Viewport = panel height - padding (top + bottom) - border (top + bottom).
        assert!(approx(px(PANEL_CHROME), px(10.0)));
    }

    #[test]
    fn test_custom_row_height_hover_geometry() {
        let at = |y: f32| hover_row_at(y, 100.0, 420.0, 0.0, 42.0, 10);
        assert_eq!(at(100.0 + 3.0 * 42.0 + 1.0), Some(3));
        assert_eq!(at(100.0 + 2.0 * 42.0), Some(2));
        assert_eq!(at(99.0), None);
        assert_eq!(at(521.0), None);
    }

    #[test]
    fn test_tab_search_filter_matching() {
        let entries = vec![
            TabEntry {
                id: 0,
                title: SharedString::from("Task list"),
                subtitle: Some(SharedString::from("editor:list-task")),
                icon: "",
                font_family: None,
                dot_color: None,
                trailing_icon: None,
            },
            TabEntry {
                id: 1,
                title: SharedString::from("Toggle preview"),
                subtitle: Some(SharedString::from("markdown:preview")),
                icon: "",
                font_family: None,
                dot_color: None,
                trailing_icon: None,
            },
        ];

        // Match by title
        let q = "task";
        let matches: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter_map(|(ix, e)| {
                let title_match = e.title.to_lowercase().contains(q);
                let sub_match = e.subtitle.as_ref().map(|s| s.to_lowercase().contains(q)).unwrap_or(false);
                if title_match || sub_match { Some(ix) } else { None }
            })
            .collect();
        assert_eq!(matches, vec![0]);

        // Match by subtitle
        let q2 = "markdown";
        let matches2: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter_map(|(ix, e)| {
                let title_match = e.title.to_lowercase().contains(q2);
                let sub_match = e.subtitle.as_ref().map(|s| s.to_lowercase().contains(q2)).unwrap_or(false);
                if title_match || sub_match { Some(ix) } else { None }
            })
            .collect();
        assert_eq!(matches2, vec![1]);

        // No matches
        let q3 = "nonexistent";
        let matches3: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter_map(|(ix, e)| {
                let title_match = e.title.to_lowercase().contains(q3);
                let sub_match = e.subtitle.as_ref().map(|s| s.to_lowercase().contains(q3)).unwrap_or(false);
                if title_match || sub_match { Some(ix) } else { None }
            })
            .collect();
        assert!(matches3.is_empty());
    }

    #[test]
    fn test_tab_entry_trailing_icon() {
        let entry_internal = TabEntry {
            id: 0,
            title: SharedString::from("Save"),
            subtitle: Some(SharedString::from("editor:save")),
            icon: "",
            font_family: None,
            dot_color: None,
            trailing_icon: Some(icons::ICON_APP_WINDOW),
        };
        let entry_plugin = TabEntry {
            id: 1,
            title: SharedString::from("Git Commit"),
            subtitle: Some(SharedString::from("obsidian-git:commit")),
            icon: "",
            font_family: None,
            dot_color: None,
            trailing_icon: Some(icons::ICON_PLUG),
        };
        assert_eq!(entry_internal.trailing_icon, Some(icons::ICON_APP_WINDOW));
        assert_eq!(entry_plugin.trailing_icon, Some(icons::ICON_PLUG));
    }

    #[test]
    fn test_search_word_range_at() {
        let text = "hello_world 123 foo.bar";
        // Selecting "hello_world"
        assert_eq!(search_word_range_at(text, 0), (0, 11));
        assert_eq!(search_word_range_at(text, 5), (0, 11));
        assert_eq!(search_word_range_at(text, 10), (0, 11));

        // Selecting space at 11
        assert_eq!(search_word_range_at(text, 11), (11, 12));

        // Selecting "123"
        assert_eq!(search_word_range_at(text, 12), (12, 15));
        assert_eq!(search_word_range_at(text, 14), (12, 15));

        // Empty text
        assert_eq!(search_word_range_at("", 0), (0, 0));
    }
