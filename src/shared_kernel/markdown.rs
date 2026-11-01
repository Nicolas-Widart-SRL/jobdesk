use pulldown_cmark::Options;

/// The extensions every reader of Markdown here turns on, so the viewer and the run logic read a text the same way.
pub fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_FOOTNOTES | Options::ENABLE_DEFINITION_LIST
}
