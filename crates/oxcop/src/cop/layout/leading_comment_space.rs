//! Port of `lib/rubocop/cop/layout/leading_comment_space.rb`.

use crate::cop::{Cop, Investigation};
use crate::processed_source::{Comment, Range};
use crate::regex;

const MSG: &str = "Missing space after `#`.";

#[derive(Default)]
pub struct LeadingCommentSpace;

impl Cop for LeadingCommentSpace {
    fn name(&self) -> &'static str {
        "Layout/LeadingCommentSpace"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        for comment in ps.comments() {
            if regex!(r"\A(?!#\+\+|#--)(#+[^#\s=])").find(comment.text).is_none() {
                continue;
            }
            let line = ps.line_of(comment.range.begin_pos);
            if line == 1 && allowed_on_first_line(cx, comment) {
                continue;
            }
            if shebang_continuation(cx, comment)
                || doxygen_comment_style(cx, comment)
                || gemfile_ruby_comment(cx, comment)
                || rbs_inline_annotation(cx, comment)
                || steep_annotation(cx, comment)
                || yard_comment_block_separator(cx, comment)
            {
                continue;
            }
            let hash_mark = Range::new(comment.range.begin_pos, comment.range.begin_pos + 1);
            cx.add_offense_with_correction(comment.range, MSG, |corrector| {
                corrector.insert_after(hash_mark, " ");
            });
        }
    }
}

fn file_basename(cx: &Investigation<'_, '_>) -> String {
    cx.processed_source
        .path()
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn allowed_on_first_line(cx: &Investigation<'_, '_>, comment: &Comment<'_>) -> bool {
    shebang(comment) || (file_basename(cx) == "config.ru" && comment.text.starts_with("#\\"))
}

fn shebang(comment: &Comment<'_>) -> bool {
    comment.text.starts_with("#!")
}

fn shebang_continuation(cx: &Investigation<'_, '_>, comment: &Comment<'_>) -> bool {
    if !shebang(comment) {
        return false;
    }
    let ps = cx.processed_source;
    let line = ps.line_of(comment.range.begin_pos);
    if line == 1 {
        return true;
    }
    let Some(previous) = ps.comment_at_line(line - 1) else {
        return false;
    };
    shebang(previous) && !cx.has_offense_at(previous.range)
}

fn doxygen_comment_style(cx: &Investigation<'_, '_>, comment: &Comment<'_>) -> bool {
    cx.cop_config_bool("AllowDoxygenCommentStyle", false) && comment.text.starts_with("#*")
}

fn gemfile_ruby_comment(cx: &Investigation<'_, '_>, comment: &Comment<'_>) -> bool {
    cx.cop_config_bool("AllowGemfileRubyComment", false)
        && file_basename(cx) == "Gemfile"
        && comment.text.starts_with("#ruby")
}

fn rbs_inline_annotation(cx: &Investigation<'_, '_>, comment: &Comment<'_>) -> bool {
    cx.cop_config_bool("AllowRBSInlineAnnotation", false)
        && regex!(r"\A(?:#:|#\[.+\]|#\|)").find(comment.text).is_some()
}

fn steep_annotation(cx: &Investigation<'_, '_>, comment: &Comment<'_>) -> bool {
    cx.cop_config_bool("AllowSteepAnnotation", false)
        && regex!(r"\A#[$:]").find(comment.text).is_some()
}

fn yard_comment_block_separator(cx: &Investigation<'_, '_>, comment: &Comment<'_>) -> bool {
    cx.cop_config_bool("AllowYARDCommentBlockSeparator", false)
        && regex!(r"\A#-\s*\z").find(comment.text).is_some()
}
