    use super::*;

    #[test]
    fn test_catalog_default_entries() {
        let state = PluginCatalogState::default();
        assert!(!state.entries.is_empty());
        assert!(state.entries.iter().any(|e| e.id == "dataview"));
        assert!(state.entries.iter().any(|e| e.id == "calendar"));
    }

    #[test]
    fn test_catalog_search_filtering() {
        let mut state = PluginCatalogState::default();
        state.entries = PluginCatalogState::default_catalog();

        state.search_query = "data".to_string();
        let filtered = state.filtered_entries();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "dataview");

        state.search_query = "liamcain".to_string();
        let author_match = state.filtered_entries();
        assert_eq!(author_match.len(), 1);
        assert_eq!(author_match[0].id, "calendar");

        state.search_query = "non-existent-plugin-query".to_string();
        assert!(state.filtered_entries().is_empty());
    }

    #[test]
    fn test_catalog_compat_filter() {
        let mut state = PluginCatalogState::default();
        state.entries = PluginCatalogState::default_catalog();

        // The registry marks dataview as engine-blocked; calendar and the rest of the
        // stub entries are outside the corpus and only pass the "All" filter.
        state.compat_filter = CatalogCompatFilter::Blocked;
        let blocked = state.filtered_entries();
        assert!(blocked.iter().any(|e| e.id == "dataview"));

        state.compat_filter = CatalogCompatFilter::Compatible;
        assert!(state.filtered_entries().is_empty());

        state.compat_filter = CatalogCompatFilter::Partial;
        assert!(state.filtered_entries().is_empty());

        // Unverified entries vanish under any specific filter but return with "All".
        state.compat_filter = CatalogCompatFilter::All;
        assert!(state.filtered_entries().iter().any(|e| e.id == "calendar"));
    }

    #[test]
    fn test_catalog_sorting_orders() {
        let mut state = PluginCatalogState::default();
        state.sort_order = CatalogSortOrder::Alphabetical;
        let alpha = state.filtered_entries();
        for i in 0..alpha.len() - 1 {
            assert!(alpha[i].name.to_lowercase() <= alpha[i + 1].name.to_lowercase());
        }

        state.sort_order = CatalogSortOrder::MostPopular;
        let popular = state.filtered_entries();
        for i in 0..popular.len() - 1 {
            assert!(popular[i].downloads >= popular[i + 1].downloads);
        }

        state.sort_order = CatalogSortOrder::RecentlyUpdated;
        let updated = state.filtered_entries();
        for i in 0..updated.len() - 1 {
            assert!(updated[i].updated >= updated[i + 1].updated);
        }
    }

    #[test]
    fn test_catalog_selection_range() {
        let mut state = PluginCatalogState::default();
        state.search_query = "hello world".to_string();
        state.cursor_pos = 0;
        state.selection_anchor = None;
        assert_eq!(state.selection_range(), None);

        // Same pos as anchor -> no selection
        state.selection_anchor = Some(0);
        assert_eq!(state.selection_range(), None);

        // Forward selection
        state.cursor_pos = 5;
        state.selection_anchor = Some(0);
        assert_eq!(state.selection_range(), Some((0, 5)));

        // Backward selection
        state.cursor_pos = 2;
        state.selection_anchor = Some(8);
        assert_eq!(state.selection_range(), Some((2, 8)));
    }
