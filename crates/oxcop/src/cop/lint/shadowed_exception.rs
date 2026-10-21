//! Port of `lib/rubocop/cop/lint/shadowed_exception.rb`.
//!
//! RuboCop resolves each rescued class with `Kernel.const_get` and compares
//! the classes with `<=>`. Here the classes come from a table of the
//! exception classes a Ruby 3.3 process running RuboCop knows (core, `Errno`
//! and the stdlib RuboCop loads); any other constant is unknown (`nil`), as
//! an undefined constant is in Ruby.

use std::cmp::Ordering;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Do not shadow rescued Exceptions.";

/// Exception classes a Ruby 3.3 process running RuboCop resolves with
/// `Kernel.const_get`: (constant path, class name, superclass name, `Errno`).
const EXCEPTION_CLASSES: &[(&str, &str, &str, Option<i64>)] = &[
    ("ArgumentError", "ArgumentError", "StandardError", None),
    ("ClosedQueueError", "ClosedQueueError", "StopIteration", None),
    ("EOFError", "EOFError", "IOError", None),
    ("EncodingError", "EncodingError", "StandardError", None),
    ("Exception", "Exception", "Object", None),
    ("FiberError", "FiberError", "StandardError", None),
    ("FloatDomainError", "FloatDomainError", "RangeError", None),
    ("FrozenError", "FrozenError", "RuntimeError", None),
    ("IOError", "IOError", "StandardError", None),
    ("IndexError", "IndexError", "StandardError", None),
    ("Interrupt", "Interrupt", "SignalException", None),
    ("KeyError", "KeyError", "IndexError", None),
    ("LoadError", "LoadError", "ScriptError", None),
    ("LocalJumpError", "LocalJumpError", "StandardError", None),
    ("NameError", "NameError", "StandardError", None),
    ("NoMatchingPatternError", "NoMatchingPatternError", "StandardError", None),
    ("NoMatchingPatternKeyError", "NoMatchingPatternKeyError", "NoMatchingPatternError", None),
    ("NoMemoryError", "NoMemoryError", "Exception", None),
    ("NoMethodError", "NoMethodError", "NameError", None),
    ("NotImplementedError", "NotImplementedError", "ScriptError", None),
    ("ParseError", "Racc::ParseError", "StandardError", None),
    ("RangeError", "RangeError", "StandardError", None),
    ("RegexpError", "RegexpError", "StandardError", None),
    ("RuntimeError", "RuntimeError", "StandardError", None),
    ("ScanError", "StringScanner::Error", "StandardError", None),
    ("ScriptError", "ScriptError", "Exception", None),
    ("SecurityError", "SecurityError", "Exception", None),
    ("SignalException", "SignalException", "Exception", None),
    ("SocketError", "SocketError", "StandardError", None),
    ("StandardError", "StandardError", "Exception", None),
    ("StopIteration", "StopIteration", "IndexError", None),
    ("SyntaxError", "SyntaxError", "ScriptError", None),
    ("SystemCallError", "SystemCallError", "StandardError", None),
    ("SystemExit", "SystemExit", "Exception", None),
    ("SystemStackError", "SystemStackError", "Exception", None),
    ("ThreadError", "ThreadError", "StandardError", None),
    ("TypeError", "TypeError", "StandardError", None),
    ("UncaughtThrowError", "UncaughtThrowError", "ArgumentError", None),
    ("ZeroDivisionError", "ZeroDivisionError", "StandardError", None),
    ("Errno::E2BIG", "Errno::E2BIG", "SystemCallError", Some(7)),
    ("Errno::EACCES", "Errno::EACCES", "SystemCallError", Some(13)),
    ("Errno::EADDRINUSE", "Errno::EADDRINUSE", "SystemCallError", Some(48)),
    ("Errno::EADDRNOTAVAIL", "Errno::EADDRNOTAVAIL", "SystemCallError", Some(49)),
    ("Errno::EADV", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EAFNOSUPPORT", "Errno::EAFNOSUPPORT", "SystemCallError", Some(47)),
    ("Errno::EAGAIN", "Errno::EAGAIN", "SystemCallError", Some(35)),
    ("Errno::EALREADY", "Errno::EALREADY", "SystemCallError", Some(37)),
    ("Errno::EAUTH", "Errno::EAUTH", "SystemCallError", Some(80)),
    ("Errno::EBADARCH", "Errno::EBADARCH", "SystemCallError", Some(86)),
    ("Errno::EBADE", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EBADEXEC", "Errno::EBADEXEC", "SystemCallError", Some(85)),
    ("Errno::EBADF", "Errno::EBADF", "SystemCallError", Some(9)),
    ("Errno::EBADFD", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EBADMACHO", "Errno::EBADMACHO", "SystemCallError", Some(88)),
    ("Errno::EBADMSG", "Errno::EBADMSG", "SystemCallError", Some(94)),
    ("Errno::EBADR", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EBADRPC", "Errno::EBADRPC", "SystemCallError", Some(72)),
    ("Errno::EBADRQC", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EBADSLT", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EBFONT", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EBUSY", "Errno::EBUSY", "SystemCallError", Some(16)),
    ("Errno::ECANCELED", "Errno::ECANCELED", "SystemCallError", Some(89)),
    ("Errno::ECAPMODE", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ECHILD", "Errno::ECHILD", "SystemCallError", Some(10)),
    ("Errno::ECHRNG", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ECOMM", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ECONNABORTED", "Errno::ECONNABORTED", "SystemCallError", Some(53)),
    ("Errno::ECONNREFUSED", "Errno::ECONNREFUSED", "SystemCallError", Some(61)),
    ("Errno::ECONNRESET", "Errno::ECONNRESET", "SystemCallError", Some(54)),
    ("Errno::EDEADLK", "Errno::EDEADLK", "SystemCallError", Some(11)),
    ("Errno::EDEADLOCK", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EDESTADDRREQ", "Errno::EDESTADDRREQ", "SystemCallError", Some(39)),
    ("Errno::EDEVERR", "Errno::EDEVERR", "SystemCallError", Some(83)),
    ("Errno::EDOM", "Errno::EDOM", "SystemCallError", Some(33)),
    ("Errno::EDOOFUS", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EDOTDOT", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EDQUOT", "Errno::EDQUOT", "SystemCallError", Some(69)),
    ("Errno::EEXIST", "Errno::EEXIST", "SystemCallError", Some(17)),
    ("Errno::EFAULT", "Errno::EFAULT", "SystemCallError", Some(14)),
    ("Errno::EFBIG", "Errno::EFBIG", "SystemCallError", Some(27)),
    ("Errno::EFTYPE", "Errno::EFTYPE", "SystemCallError", Some(79)),
    ("Errno::EHOSTDOWN", "Errno::EHOSTDOWN", "SystemCallError", Some(64)),
    ("Errno::EHOSTUNREACH", "Errno::EHOSTUNREACH", "SystemCallError", Some(65)),
    ("Errno::EHWPOISON", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EIDRM", "Errno::EIDRM", "SystemCallError", Some(90)),
    ("Errno::EILSEQ", "Errno::EILSEQ", "SystemCallError", Some(92)),
    ("Errno::EINPROGRESS", "Errno::EINPROGRESS", "SystemCallError", Some(36)),
    ("Errno::EINTR", "Errno::EINTR", "SystemCallError", Some(4)),
    ("Errno::EINVAL", "Errno::EINVAL", "SystemCallError", Some(22)),
    ("Errno::EIO", "Errno::EIO", "SystemCallError", Some(5)),
    ("Errno::EIPSEC", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EISCONN", "Errno::EISCONN", "SystemCallError", Some(56)),
    ("Errno::EISDIR", "Errno::EISDIR", "SystemCallError", Some(21)),
    ("Errno::EISNAM", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EKEYEXPIRED", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EKEYREJECTED", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EKEYREVOKED", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EL2HLT", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EL2NSYNC", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EL3HLT", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EL3RST", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ELAST", "Errno::ELAST", "SystemCallError", Some(106)),
    ("Errno::ELIBACC", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ELIBBAD", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ELIBEXEC", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ELIBMAX", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ELIBSCN", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ELNRNG", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ELOOP", "Errno::ELOOP", "SystemCallError", Some(62)),
    ("Errno::EMEDIUMTYPE", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EMFILE", "Errno::EMFILE", "SystemCallError", Some(24)),
    ("Errno::EMLINK", "Errno::EMLINK", "SystemCallError", Some(31)),
    ("Errno::EMSGSIZE", "Errno::EMSGSIZE", "SystemCallError", Some(40)),
    ("Errno::EMULTIHOP", "Errno::EMULTIHOP", "SystemCallError", Some(95)),
    ("Errno::ENAMETOOLONG", "Errno::ENAMETOOLONG", "SystemCallError", Some(63)),
    ("Errno::ENAVAIL", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ENEEDAUTH", "Errno::ENEEDAUTH", "SystemCallError", Some(81)),
    ("Errno::ENETDOWN", "Errno::ENETDOWN", "SystemCallError", Some(50)),
    ("Errno::ENETRESET", "Errno::ENETRESET", "SystemCallError", Some(52)),
    ("Errno::ENETUNREACH", "Errno::ENETUNREACH", "SystemCallError", Some(51)),
    ("Errno::ENFILE", "Errno::ENFILE", "SystemCallError", Some(23)),
    ("Errno::ENOANO", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ENOATTR", "Errno::ENOATTR", "SystemCallError", Some(93)),
    ("Errno::ENOBUFS", "Errno::ENOBUFS", "SystemCallError", Some(55)),
    ("Errno::ENOCSI", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ENODATA", "Errno::ENODATA", "SystemCallError", Some(96)),
    ("Errno::ENODEV", "Errno::ENODEV", "SystemCallError", Some(19)),
    ("Errno::ENOENT", "Errno::ENOENT", "SystemCallError", Some(2)),
    ("Errno::ENOEXEC", "Errno::ENOEXEC", "SystemCallError", Some(8)),
    ("Errno::ENOKEY", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ENOLCK", "Errno::ENOLCK", "SystemCallError", Some(77)),
    ("Errno::ENOLINK", "Errno::ENOLINK", "SystemCallError", Some(97)),
    ("Errno::ENOMEDIUM", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ENOMEM", "Errno::ENOMEM", "SystemCallError", Some(12)),
    ("Errno::ENOMSG", "Errno::ENOMSG", "SystemCallError", Some(91)),
    ("Errno::ENONET", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ENOPKG", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ENOPOLICY", "Errno::ENOPOLICY", "SystemCallError", Some(103)),
    ("Errno::ENOPROTOOPT", "Errno::ENOPROTOOPT", "SystemCallError", Some(42)),
    ("Errno::ENOSPC", "Errno::ENOSPC", "SystemCallError", Some(28)),
    ("Errno::ENOSR", "Errno::ENOSR", "SystemCallError", Some(98)),
    ("Errno::ENOSTR", "Errno::ENOSTR", "SystemCallError", Some(99)),
    ("Errno::ENOSYS", "Errno::ENOSYS", "SystemCallError", Some(78)),
    ("Errno::ENOTBLK", "Errno::ENOTBLK", "SystemCallError", Some(15)),
    ("Errno::ENOTCAPABLE", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ENOTCONN", "Errno::ENOTCONN", "SystemCallError", Some(57)),
    ("Errno::ENOTDIR", "Errno::ENOTDIR", "SystemCallError", Some(20)),
    ("Errno::ENOTEMPTY", "Errno::ENOTEMPTY", "SystemCallError", Some(66)),
    ("Errno::ENOTNAM", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ENOTRECOVERABLE", "Errno::ENOTRECOVERABLE", "SystemCallError", Some(104)),
    ("Errno::ENOTSOCK", "Errno::ENOTSOCK", "SystemCallError", Some(38)),
    ("Errno::ENOTSUP", "Errno::ENOTSUP", "SystemCallError", Some(45)),
    ("Errno::ENOTTY", "Errno::ENOTTY", "SystemCallError", Some(25)),
    ("Errno::ENOTUNIQ", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ENXIO", "Errno::ENXIO", "SystemCallError", Some(6)),
    ("Errno::EOPNOTSUPP", "Errno::EOPNOTSUPP", "SystemCallError", Some(102)),
    ("Errno::EOVERFLOW", "Errno::EOVERFLOW", "SystemCallError", Some(84)),
    ("Errno::EOWNERDEAD", "Errno::EOWNERDEAD", "SystemCallError", Some(105)),
    ("Errno::EPERM", "Errno::EPERM", "SystemCallError", Some(1)),
    ("Errno::EPFNOSUPPORT", "Errno::EPFNOSUPPORT", "SystemCallError", Some(46)),
    ("Errno::EPIPE", "Errno::EPIPE", "SystemCallError", Some(32)),
    ("Errno::EPROCLIM", "Errno::EPROCLIM", "SystemCallError", Some(67)),
    ("Errno::EPROCUNAVAIL", "Errno::EPROCUNAVAIL", "SystemCallError", Some(76)),
    ("Errno::EPROGMISMATCH", "Errno::EPROGMISMATCH", "SystemCallError", Some(75)),
    ("Errno::EPROGUNAVAIL", "Errno::EPROGUNAVAIL", "SystemCallError", Some(74)),
    ("Errno::EPROTO", "Errno::EPROTO", "SystemCallError", Some(100)),
    ("Errno::EPROTONOSUPPORT", "Errno::EPROTONOSUPPORT", "SystemCallError", Some(43)),
    ("Errno::EPROTOTYPE", "Errno::EPROTOTYPE", "SystemCallError", Some(41)),
    ("Errno::EPWROFF", "Errno::EPWROFF", "SystemCallError", Some(82)),
    ("Errno::EQFULL", "Errno::ELAST", "SystemCallError", Some(106)),
    ("Errno::ERANGE", "Errno::ERANGE", "SystemCallError", Some(34)),
    ("Errno::EREMCHG", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EREMOTE", "Errno::EREMOTE", "SystemCallError", Some(71)),
    ("Errno::EREMOTEIO", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ERESTART", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ERFKILL", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EROFS", "Errno::EROFS", "SystemCallError", Some(30)),
    ("Errno::ERPCMISMATCH", "Errno::ERPCMISMATCH", "SystemCallError", Some(73)),
    ("Errno::ESHLIBVERS", "Errno::ESHLIBVERS", "SystemCallError", Some(87)),
    ("Errno::ESHUTDOWN", "Errno::ESHUTDOWN", "SystemCallError", Some(58)),
    ("Errno::ESOCKTNOSUPPORT", "Errno::ESOCKTNOSUPPORT", "SystemCallError", Some(44)),
    ("Errno::ESPIPE", "Errno::ESPIPE", "SystemCallError", Some(29)),
    ("Errno::ESRCH", "Errno::ESRCH", "SystemCallError", Some(3)),
    ("Errno::ESRMNT", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ESTALE", "Errno::ESTALE", "SystemCallError", Some(70)),
    ("Errno::ESTRPIPE", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::ETIME", "Errno::ETIME", "SystemCallError", Some(101)),
    ("Errno::ETIMEDOUT", "Errno::ETIMEDOUT", "SystemCallError", Some(60)),
    ("Errno::ETOOMANYREFS", "Errno::ETOOMANYREFS", "SystemCallError", Some(59)),
    ("Errno::ETXTBSY", "Errno::ETXTBSY", "SystemCallError", Some(26)),
    ("Errno::EUCLEAN", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EUNATCH", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::EUSERS", "Errno::EUSERS", "SystemCallError", Some(68)),
    ("Errno::EWOULDBLOCK", "Errno::EAGAIN", "SystemCallError", Some(35)),
    ("Errno::EXDEV", "Errno::EXDEV", "SystemCallError", Some(18)),
    ("Errno::EXFULL", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Errno::NOERROR", "Errno::NOERROR", "SystemCallError", Some(0)),
    ("Timeout::Error", "Timeout::Error", "RuntimeError", None),
    ("Timeout::ExitException", "Timeout::ExitException", "Exception", None),
    ("IO::EAGAINWaitReadable", "IO::EAGAINWaitReadable", "Errno::EAGAIN", None),
    ("IO::EAGAINWaitWritable", "IO::EAGAINWaitWritable", "Errno::EAGAIN", None),
    ("IO::EINPROGRESSWaitWritable", "IO::EINPROGRESSWaitWritable", "Errno::EINPROGRESS", None),
    ("IO::TimeoutError", "IO::TimeoutError", "IOError", None),
    ("Psych::SyntaxError", "Psych::SyntaxError", "Psych::Exception", None),
    ("JSON::ParserError", "JSON::ParserError", "JSON::JSONError", None),
    ("Encoding::CompatibilityError", "Encoding::CompatibilityError", "EncodingError", None),
    ("Encoding::UndefinedConversionError", "Encoding::UndefinedConversionError", "EncodingError", None),
    ("Encoding::InvalidByteSequenceError", "Encoding::InvalidByteSequenceError", "EncodingError", None),
    ("Encoding::ConverterNotFoundError", "Encoding::ConverterNotFoundError", "EncodingError", None),
    ("Ractor::Error", "Ractor::Error", "RuntimeError", None),
    ("Ractor::RemoteError", "Ractor::RemoteError", "Ractor::Error", None),
    ("Ractor::MovedError", "Ractor::MovedError", "Ractor::Error", None),
    ("Ractor::ClosedError", "Ractor::ClosedError", "StopIteration", None),
    ("Ractor::IsolationError", "Ractor::IsolationError", "Ractor::Error", None),
    ("Gem::LoadError", "Gem::LoadError", "LoadError", None),
    ("Psych::Exception", "Psych::Exception", "RuntimeError", None),
    ("JSON::JSONError", "JSON::JSONError", "StandardError", None),
];

/// A resolved class: its name (`None` for an unknown constant).
type Class = Option<&'static str>;

fn const_get(source: &str) -> Class {
    let path = source.strip_prefix("::").unwrap_or(source);
    EXCEPTION_CLASSES
        .iter()
        .find(|(p, ..)| *p == path)
        .map(|(_, name, ..)| *name)
}

fn class_entry(name: &str) -> Option<&'static (&'static str, &'static str, &'static str, Option<i64>)> {
    EXCEPTION_CLASSES.iter().find(|(_, n, ..)| *n == name)
}

fn superclass(name: &str) -> Option<&'static str> {
    class_entry(name).map(|(_, _, superclass, _)| *superclass)
}

/// Whether `a` is `b` or inherits from it.
fn descends_from(a: &str, b: &str) -> bool {
    let mut current = Some(a);
    while let Some(name) = current {
        if name == b {
            return true;
        }
        current = superclass(name);
    }
    false
}

/// `Module#<=>`.
fn class_cmp(a: &str, b: &str) -> Option<Ordering> {
    if a == b {
        Some(Ordering::Equal)
    } else if descends_from(a, b) {
        Some(Ordering::Less)
    } else if descends_from(b, a) {
        Some(Ordering::Greater)
    } else {
        None
    }
}

/// `<=>` between two group elements (`nil <=> nil` is 0).
fn element_cmp(a: Class, b: Class) -> Option<Ordering> {
    match (a, b) {
        (Some(a), Some(b)) => class_cmp(a, b),
        (None, None) => Some(Ordering::Equal),
        _ => None,
    }
}

/// `Array#<=>`.
fn array_cmp(x: &[Class], y: &[Class]) -> Option<Ordering> {
    for (a, b) in x.iter().zip(y) {
        match element_cmp(*a, *b) {
            Some(Ordering::Equal) => {}
            other => return other,
        }
    }
    Some(x.len().cmp(&y.len()))
}

#[derive(Default)]
pub struct ShadowedException;

impl Cop for ShadowedException {
    fn name(&self) -> &'static str {
        "Lint/ShadowedException"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Rescue]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        // `rescue_modifier?(node)` is always false for a `rescue` node: it
        // only answers for `resbody` nodes.
        let rescues = node.resbody_branches();
        let rescued_groups = rescued_groups_for(&rescues);

        let rescue_group_rescues_multiple_levels = rescued_groups
            .iter()
            .any(|group| contains_multiple_levels_of_exceptions(group));
        if !rescue_group_rescues_multiple_levels && sorted(&rescued_groups) {
            return;
        }

        if let Some(range) = offense_range(&rescues) {
            cx.add_offense(range, MSG);
        }
    }
}

fn offense_range(rescues: &[Node<'_>]) -> Option<Range> {
    let shadowing_rescue = find_shadowing_rescue(rescues)?;
    let expression = shadowing_rescue.source_range();
    Some(range_between(expression.begin_pos, expression.end_pos))
}

fn rescued_groups_for(rescues: &[Node<'_>]) -> Vec<Vec<Class>> {
    rescues.iter().map(|group| evaluate_exceptions(*group)).collect()
}

fn contains_multiple_levels_of_exceptions(group: &[Class]) -> bool {
    if group.len() > 1 && group.contains(&Some("Exception")) {
        return true;
    }

    (0..group.len()).any(|i| {
        (i + 1..group.len()).any(|j| compare_exceptions(group[i], group[j]))
    })
}

fn compare_exceptions(exception: Class, other_exception: Class) -> bool {
    if system_call_err(exception) && system_call_err(other_exception) {
        let errno = |c: Class| c.and_then(class_entry).and_then(|(.., errno)| *errno);
        errno(exception) != errno(other_exception)
            && element_cmp(exception, other_exception).is_some()
    } else {
        match (exception, other_exception) {
            (Some(a), Some(b)) => class_cmp(a, b).is_some(),
            _ => false,
        }
    }
}

fn system_call_err(error: Class) -> bool {
    error.is_some_and(|e| superclass(e) == Some("SystemCallError"))
}

fn evaluate_exceptions(group: Node<'_>) -> Vec<Class> {
    let rescued_exceptions = group.exceptions();
    if rescued_exceptions.is_empty() {
        vec![Some("StandardError")]
    } else {
        rescued_exceptions
            .iter()
            .map(|exception| const_get(exception.source()))
            .collect()
    }
}

fn sorted(rescued_groups: &[Vec<Class>]) -> bool {
    rescued_groups.windows(2).all(|pair| {
        let (x, y) = (&pair[0], &pair[1]);
        if x.contains(&Some("Exception")) {
            false
        } else if y.contains(&Some("Exception"))
            || x.iter().all(Option::is_none)
            || y.iter().all(Option::is_none)
        {
            true
        } else {
            array_cmp(x, y).unwrap_or(Ordering::Equal) != Ordering::Greater
        }
    })
}

fn find_shadowing_rescue<'a>(rescues: &[Node<'a>]) -> Option<Node<'a>> {
    let rescued_groups = rescued_groups_for(rescues);
    for (group, res) in rescued_groups.iter().zip(rescues) {
        if contains_multiple_levels_of_exceptions(group) {
            return Some(*res);
        }
    }
    for (i, pair) in rescued_groups.windows(2).enumerate() {
        if !sorted(pair) {
            return Some(rescues[i]);
        }
    }
    None
}
