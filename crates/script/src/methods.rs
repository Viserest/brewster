//! Built-in functions and methods: strings, lists, options, nodes (`root.tree`) and
//! `root.plugin`.

use crate::interp::{Interpreter, R};
use crate::value::*;

use plugin::{AppKey, Plugin, PluginError, Uuid};
use tree::{Node, NodeError, QueryError};

const MAX_STRING: usize = 4 * 1024 * 1024;

fn argc(it: &Interpreter, name: &str, args: &[Value], n: usize) -> R<()> {
    if args.len() == n {
        Ok(())
    } else {
        Err(it.rt(format!(
            "`{}` takes {} argument{} but {} {} given",
            name,
            n,
            if n == 1 { "" } else { "s" },
            args.len(),
            if args.len() == 1 { "was" } else { "were" }
        )))
    }
}

fn str_arg(it: &Interpreter, name: &str, args: &[Value], i: usize) -> R<String> {
    match &args[i] {
        Value::Str(s) => Ok(s.clone()),
        Value::Tag(s) => Ok(s.clone()),
        other => Err(it.rt(format!(
            "`{}` expects a String for argument {}, found a {}",
            name,
            i + 1,
            other.type_name()
        ))),
    }
}

fn int_arg(it: &Interpreter, name: &str, args: &[Value], i: usize) -> R<i64> {
    match &args[i] {
        Value::Int(n) => Ok(*n),
        other => Err(it.rt(format!(
            "`{}` expects an Int for argument {}, found a {}",
            name,
            i + 1,
            other.type_name()
        ))),
    }
}

fn plugin_arg(it: &Interpreter, name: &str, args: &[Value], i: usize) -> R<Plugin> {
    match &args[i] {
        Value::Plugin(p) => Ok(p.clone()),
        other => Err(it.rt(format!(
            "`{}` expects a Plugin for argument {}, found a {} (use Plugin::source, Plugin::compiled or Plugin::url)",
            name,
            i + 1,
            other.type_name()
        ))),
    }
}

fn uuid_arg(it: &Interpreter, name: &str, args: &[Value], i: usize) -> R<Uuid> {
    match &args[i] {
        Value::Uuid(u) => Ok(*u),
        Value::Str(s) => Uuid::parse(s).ok_or_else(|| it.rt(format!("`{}` is not a Uuid", s))),
        other => Err(it.rt(format!(
            "`{}` expects a Uuid for argument {}, found a {}",
            name,
            i + 1,
            other.type_name()
        ))),
    }
}

fn plugin_error(e: PluginError) -> Value {
    Value::error(e.kind(), e.to_string())
}

fn query_error(e: &QueryError) -> Value {
    let kind = match e {
        QueryError::Empty => "Empty",
        QueryError::Invalid(_) => "Invalid",
        QueryError::NotFound(_) => "NotFound",
    };
    Value::error(kind, e.to_string())
}

fn node_error(e: &NodeError) -> Value {
    let kind = match e {
        NodeError::UnknownTag(_) => "UnknownTag",
        NodeError::NotAContainer(_) => "NotAContainer",
        NodeError::Cycle => "Cycle",
    };
    Value::error(kind, e.to_string())
}

fn to_result<T>(r: Result<T, Value>, f: impl FnOnce(T) -> Value) -> Value {
    match r {
        Ok(v) => Value::ok(f(v)),
        Err(e) => Value::err(e),
    }
}

// ── fields ──────────────────────────────────────────────────────────

pub fn field(it: &mut Interpreter, recv: &Value, name: &str) -> R<Value> {
    match (recv, name) {
        (Value::Root, "plugin") => Ok(Value::RootPlugin),
        (Value::Root, "tree") => Ok(Value::Node(it.root.clone())),
        (Value::Root, "url") => Ok(Value::Str(it.svc.url.to_string())),
        (Value::Error { kind, .. }, "kind") => Ok(Value::Str(kind.clone())),
        (Value::Error { message, .. }, "message") => Ok(Value::Str(message.clone())),
        (Value::Node(n), "tag") => Ok(Value::Str(n.tag())),
        (Value::Node(n), "text") => Ok(Value::Str(n.text())),
        (Value::Node(n), "id") => Ok(Value::Int(n.id() as i64)),
        (Value::Plugin(p), "kind") => Ok(Value::Str(plugin_kind(p).to_string())),
        _ => Err(it.rt(format!("a {} has no field `{}`", recv.type_name(), name))),
    }
}

fn plugin_kind(p: &Plugin) -> &'static str {
    match p {
        Plugin::Compiled(_) => "compiled",
        Plugin::Source(_) => "source",
        Plugin::Url(_) => "url",
    }
}

// ── free functions ──────────────────────────────────────────────────

pub fn builtin(it: &mut Interpreter, name: &str, args: Vec<Value>) -> R<Value> {
    match name {
        "Some" => {
            argc(it, name, &args, 1)?;
            Ok(Value::some(args.into_iter().next().expect("one argument")))
        }
        "Ok" => {
            argc(it, name, &args, 1)?;
            Ok(Value::ok(args.into_iter().next().expect("one argument")))
        }
        "Err" => {
            argc(it, name, &args, 1)?;
            Ok(Value::err(args.into_iter().next().expect("one argument")))
        }
        "log" => {
            let line: Vec<String> = args.iter().map(|a| a.display()).collect();
            it.push_log(line.join(" "));
            Ok(Value::Unit)
        }
        "str" => {
            argc(it, name, &args, 1)?;
            Ok(Value::Str(args[0].display()))
        }
        "int" => {
            argc(it, name, &args, 1)?;
            Ok(match &args[0] {
                Value::Int(i) => Value::ok(Value::Int(*i)),
                Value::Float(f) if f.is_finite() => Value::ok(Value::Int(f.trunc() as i64)),
                Value::Str(s) => match s.trim().parse::<i64>() {
                    Ok(i) => Value::ok(Value::Int(i)),
                    Err(_) => {
                        Value::err(Value::error("ParseError", format!("`{}` is not an Int", s)))
                    }
                },
                other => Value::err(Value::error(
                    "ParseError",
                    format!("a {} is not an Int", other.type_name()),
                )),
            })
        }
        "float" => {
            argc(it, name, &args, 1)?;
            Ok(match &args[0] {
                Value::Int(i) => Value::ok(Value::Float(*i as f64)),
                Value::Float(f) => Value::ok(Value::Float(*f)),
                Value::Str(s) => match s.trim().parse::<f64>() {
                    Ok(f) if f.is_finite() => Value::ok(Value::Float(f)),
                    _ => Value::err(Value::error(
                        "ParseError",
                        format!("`{}` is not a Float", s),
                    )),
                },
                other => Value::err(Value::error(
                    "ParseError",
                    format!("a {} is not a Float", other.type_name()),
                )),
            })
        }
        "len" => {
            argc(it, name, &args, 1)?;
            match &args[0] {
                Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
                Value::List(l) => Ok(Value::Int(l.len() as i64)),
                other => Err(it.rt(format!(
                    "`len` needs a String or List, found a {}",
                    other.type_name()
                ))),
            }
        }
        _ => Err(it.rt(format!("unknown function `{}`", name))),
    }
}

pub fn path_call(it: &mut Interpreter, path: &[String], args: Vec<Value>) -> R<Value> {
    let full = path.join("::");
    match path {
        [t, f] if t == "Plugin" => match f.as_str() {
            "source" => {
                argc(it, &full, &args, 1)?;
                let text = str_arg(it, &full, &args, 0)?;
                Ok(Value::Plugin(Plugin::Source(text)))
            }
            "url" => {
                argc(it, &full, &args, 1)?;
                let u = str_arg(it, &full, &args, 0)?;
                Ok(Value::Plugin(Plugin::Url(u)))
            }
            "compiled" => {
                argc(it, &full, &args, 1)?;
                let h = str_arg(it, &full, &args, 0)?;
                Ok(match unhex(&h) {
                    Some(b) => Value::ok(Value::Plugin(Plugin::Compiled(b))),
                    None => Value::err(Value::error(
                        "Compile",
                        "compiled plugins are given as hexadecimal text",
                    )),
                })
            }
            "compile" => {
                argc(it, &full, &args, 1)?;
                let text = str_arg(it, &full, &args, 0)?;
                Ok(to_result(
                    Plugin::compile(&text).map_err(plugin_error),
                    Value::Plugin,
                ))
            }
            _ => Err(it.rt(format!("unknown function `{}`", full))),
        },
        [t, f] if t == "Uuid" && f == "parse" => {
            argc(it, &full, &args, 1)?;
            let s = str_arg(it, &full, &args, 0)?;
            Ok(match Uuid::parse(&s) {
                Some(u) => Value::ok(Value::Uuid(u)),
                None => Value::err(Value::error("ParseError", format!("`{}` is not a Uuid", s))),
            })
        }
        _ => Err(it.rt(format!("unknown function `{}`", full))),
    }
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    plugin::unhex(s)
}

// ── methods ─────────────────────────────────────────────────────────

pub fn method(it: &mut Interpreter, recv: Value, name: &str, args: Vec<Value>) -> R<Value> {
    match &recv {
        Value::Node(n) => node_method(it, n, name, args),
        Value::RootPlugin => plugin_api(it, name, args),
        Value::Template(t) => match name {
            "build" => {
                argc(it, "build", &args, 0)?;
                let t = t.clone();
                Ok(Value::Node(it.build_template(&t)?))
            }
            _ => no_method(it, &recv, name),
        },
        Value::Str(s) => str_method(it, s, name, &args),
        Value::List(l) => list_method(it, l, name, &args),
        Value::Option(o) => option_method(it, &recv, o, name, &args),
        Value::Result(r) => result_method(it, &recv, r, name, &args),
        Value::Plugin(p) => match name {
            "kind" => Ok(Value::Str(plugin_kind(p).to_string())),
            _ => no_method(it, &recv, name),
        },
        Value::Func(_) => no_method(it, &recv, name),
        _ => no_method(it, &recv, name),
    }
}

fn no_method(it: &Interpreter, recv: &Value, name: &str) -> R<Value> {
    Err(it.rt(format!("a {} has no method `{}`", recv.type_name(), name)))
}

fn str_method(it: &mut Interpreter, s: &str, name: &str, args: &[Value]) -> R<Value> {
    let full = format!("String.{}", name);
    match name {
        "len" => {
            argc(it, &full, args, 0)?;
            Ok(Value::Int(s.chars().count() as i64))
        }
        "is_empty" => {
            argc(it, &full, args, 0)?;
            Ok(Value::Bool(s.is_empty()))
        }
        "upper" => {
            argc(it, &full, args, 0)?;
            Ok(Value::Str(s.to_uppercase()))
        }
        "lower" => {
            argc(it, &full, args, 0)?;
            Ok(Value::Str(s.to_lowercase()))
        }
        "trim" => {
            argc(it, &full, args, 0)?;
            Ok(Value::Str(s.trim().to_string()))
        }
        "contains" => {
            argc(it, &full, args, 1)?;
            Ok(Value::Bool(
                s.contains(str_arg(it, &full, args, 0)?.as_str()),
            ))
        }
        "starts_with" => {
            argc(it, &full, args, 1)?;
            Ok(Value::Bool(
                s.starts_with(str_arg(it, &full, args, 0)?.as_str()),
            ))
        }
        "ends_with" => {
            argc(it, &full, args, 1)?;
            Ok(Value::Bool(
                s.ends_with(str_arg(it, &full, args, 0)?.as_str()),
            ))
        }
        "replace" => {
            argc(it, &full, args, 2)?;
            let from = str_arg(it, &full, args, 0)?;
            let to = str_arg(it, &full, args, 1)?;
            if from.is_empty() {
                return Err(it.rt("`replace` needs a non-empty text to look for"));
            }
            let out = s.replace(&from, &to);
            if out.len() > MAX_STRING {
                return Err(it.rt("the string became too large"));
            }
            Ok(Value::Str(out))
        }
        "split" => {
            argc(it, &full, args, 1)?;
            let sep = str_arg(it, &full, args, 0)?;
            let parts: Vec<Value> = if sep.is_empty() {
                s.chars().map(|c| Value::Str(c.to_string())).collect()
            } else {
                s.split(sep.as_str())
                    .map(|p| Value::Str(p.to_string()))
                    .collect()
            };
            Ok(Value::List(parts))
        }
        "repeat" => {
            argc(it, &full, args, 1)?;
            let n = int_arg(it, &full, args, 0)?;
            if n < 0 || (n as u128) * (s.len() as u128) > MAX_STRING as u128 {
                return Err(it.rt("`repeat` count is negative or makes the string too large"));
            }
            Ok(Value::Str(s.repeat(n as usize)))
        }
        _ => no_method(it, &Value::Str(s.to_string()), name),
    }
}

fn list_method(it: &mut Interpreter, l: &[Value], name: &str, args: &[Value]) -> R<Value> {
    let full = format!("List.{}", name);
    match name {
        "len" => {
            argc(it, &full, args, 0)?;
            Ok(Value::Int(l.len() as i64))
        }
        "is_empty" => {
            argc(it, &full, args, 0)?;
            Ok(Value::Bool(l.is_empty()))
        }
        "first" => {
            argc(it, &full, args, 0)?;
            Ok(l.first().cloned().map_or_else(Value::none, Value::some))
        }
        "last" => {
            argc(it, &full, args, 0)?;
            Ok(l.last().cloned().map_or_else(Value::none, Value::some))
        }
        "get" => {
            argc(it, &full, args, 1)?;
            let i = int_arg(it, &full, args, 0)?;
            Ok(usize::try_from(i)
                .ok()
                .and_then(|i| l.get(i))
                .cloned()
                .map_or_else(Value::none, Value::some))
        }
        "contains" => {
            argc(it, &full, args, 1)?;
            Ok(Value::Bool(l.iter().any(|v| values_equal(v, &args[0]))))
        }
        "join" => {
            argc(it, &full, args, 1)?;
            let sep = str_arg(it, &full, args, 0)?;
            let parts: Vec<String> = l.iter().map(|v| v.display()).collect();
            Ok(Value::Str(parts.join(&sep)))
        }
        _ => no_method(it, &Value::List(l.to_vec()), name),
    }
}

fn option_method(
    it: &mut Interpreter,
    recv: &Value,
    o: &Option<Box<Value>>,
    name: &str,
    args: &[Value],
) -> R<Value> {
    let full = format!("Option.{}", name);
    match name {
        "is_some" => {
            argc(it, &full, args, 0)?;
            Ok(Value::Bool(o.is_some()))
        }
        "is_none" => {
            argc(it, &full, args, 0)?;
            Ok(Value::Bool(o.is_none()))
        }
        "unwrap" => {
            argc(it, &full, args, 0)?;
            match o {
                Some(v) => Ok((**v).clone()),
                None => Err(it.rt("called `unwrap` on None")),
            }
        }
        "unwrap_or" => {
            argc(it, &full, args, 1)?;
            Ok(match o {
                Some(v) => (**v).clone(),
                None => args[0].clone(),
            })
        }
        _ => no_method(it, recv, name),
    }
}

fn result_method(
    it: &mut Interpreter,
    recv: &Value,
    r: &Result<Box<Value>, Box<Value>>,
    name: &str,
    args: &[Value],
) -> R<Value> {
    let full = format!("Result.{}", name);
    match name {
        "is_ok" => {
            argc(it, &full, args, 0)?;
            Ok(Value::Bool(r.is_ok()))
        }
        "is_err" => {
            argc(it, &full, args, 0)?;
            Ok(Value::Bool(r.is_err()))
        }
        "unwrap" => {
            argc(it, &full, args, 0)?;
            match r {
                Ok(v) => Ok((**v).clone()),
                Err(e) => Err(it.rt(format!("called `unwrap` on Err: {}", e.display()))),
            }
        }
        "unwrap_or" => {
            argc(it, &full, args, 1)?;
            Ok(match r {
                Ok(v) => (**v).clone(),
                Err(_) => args[0].clone(),
            })
        }
        _ => no_method(it, recv, name),
    }
}

// ── nodes: root.tree ────────────────────────────────────────────────

fn node_arg(it: &mut Interpreter, name: &str, args: &[Value], i: usize) -> R<Node> {
    match &args[i] {
        Value::Node(n) => Ok(n.clone()),
        Value::Template(t) => {
            let t = t.clone();
            it.build_template(&t)
        }
        other => Err(it.rt(format!(
            "`{}` expects an Element for argument {}, found a {}",
            name,
            i + 1,
            other.type_name()
        ))),
    }
}

fn node_method(it: &mut Interpreter, n: &Node, name: &str, args: Vec<Value>) -> R<Value> {
    let full = format!("Element.{}", name);
    match name {
        "query" => {
            argc(it, &full, &args, 1)?;
            let sel = str_arg(it, &full, &args, 0)?;
            Ok(to_result(
                n.query(&sel).map_err(|e| query_error(&e)),
                Value::Node,
            ))
        }
        "query_all" => {
            argc(it, &full, &args, 1)?;
            let sel = str_arg(it, &full, &args, 0)?;
            Ok(to_result(
                n.query_all(&sel).map_err(|e| query_error(&e)),
                |v| Value::List(v.into_iter().map(Value::Node).collect()),
            ))
        }
        "create" => {
            argc(it, &full, &args, 1)?;
            let tag = str_arg(it, &full, &args, 0)?;
            Ok(to_result(
                Node::create(&tag).map_err(|e| node_error(&e)),
                Value::Node,
            ))
        }
        "append" => {
            argc(it, &full, &args, 1)?;
            let child = node_arg(it, &full, &args, 0)?;
            Ok(to_result(
                n.append(&child).map_err(|e| node_error(&e)),
                Value::Node,
            ))
        }
        "remove" => {
            argc(it, &full, &args, 0)?;
            n.remove();
            Ok(Value::Unit)
        }
        "children" => {
            argc(it, &full, &args, 0)?;
            Ok(Value::List(
                n.children().into_iter().map(Value::Node).collect(),
            ))
        }
        "parent" => {
            argc(it, &full, &args, 0)?;
            Ok(n.parent()
                .map_or_else(Value::none, |p| Value::some(Value::Node(p))))
        }
        "tag" => {
            argc(it, &full, &args, 0)?;
            Ok(Value::Str(n.tag()))
        }
        "text" => {
            argc(it, &full, &args, 0)?;
            Ok(Value::Str(n.text()))
        }
        "id" => {
            argc(it, &full, &args, 0)?;
            Ok(Value::Int(n.id() as i64))
        }
        "set_text" => {
            argc(it, &full, &args, 1)?;
            let text = args[0].display();
            let slot = match n.tag().as_str() {
                "link" | "input" => 1,
                "box" | "root" => {
                    return Ok(Value::err(Value::error(
                        "NoText",
                        format!(
                            "`{}` has no text of its own; set the text of its children",
                            n.tag()
                        ),
                    )));
                }
                _ => 0,
            };
            n.set_arg(slot, &text);
            Ok(Value::ok(Value::Node(n.clone())))
        }
        "get_style" => {
            argc(it, &full, &args, 1)?;
            let key = str_arg(it, &full, &args, 0)?;
            Ok(n.style_get(&key)
                .map_or_else(Value::none, |v| Value::some(Value::Str(v.join(",")))))
        }
        "set_style" => {
            argc(it, &full, &args, 2)?;
            let key = str_arg(it, &full, &args, 0)?;
            if key.is_empty()
                || !key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(it.rt(format!("`{}` is not a style name", key)));
            }
            let val = args[1].display();
            let values: Vec<String> = if val.trim().is_empty() {
                Vec::new()
            } else {
                val.split(',').map(|v| v.trim().to_string()).collect()
            };
            n.set_style(&key, values);
            Ok(Value::Unit)
        }
        "get_arg" => {
            argc(it, &full, &args, 1)?;
            let i = int_arg(it, &full, &args, 0)?;
            Ok(usize::try_from(i)
                .ok()
                .and_then(|i| n.args().get(i).cloned())
                .map_or_else(Value::none, |s| Value::some(Value::Str(s))))
        }
        "set_arg" => {
            argc(it, &full, &args, 2)?;
            let i = int_arg(it, &full, &args, 0)?;
            if !(0..64).contains(&i) {
                return Err(it.rt("argument index must be between 0 and 63"));
            }
            n.set_arg(i as usize, &args[1].display());
            Ok(Value::Unit)
        }
        _ => no_method(it, &Value::Node(n.clone()), name),
    }
}

// ── root.plugin ─────────────────────────────────────────────────────

fn plugin_api(it: &mut Interpreter, name: &str, args: Vec<Value>) -> R<Value> {
    let full = format!("root.plugin.{}", name);
    let authority = it.svc.url.authority().to_string();
    match name {
        "oneshot" => {
            argc(it, &full, &args, 1)?;
            let p = plugin_arg(it, &full, &args, 0)?;
            Ok(to_result(
                it.run_plugin_now(&p).map_err(plugin_error),
                |_| Value::Unit,
            ))
        }
        "install" => {
            argc(it, &full, &args, 2)?;
            let app = str_arg(it, &full, &args, 0)?;
            let p = plugin_arg(it, &full, &args, 1)?;
            let r = AppKey::new(&authority, &app).and_then(|key| {
                it.svc
                    .registry
                    .borrow_mut()
                    .install(key, p, &it.svc.url, &*it.svc.loader)
            });
            Ok(to_result(r.map_err(plugin_error), Value::Uuid))
        }
        "list" => {
            argc(it, &full, &args, 1)?;
            let app = str_arg(it, &full, &args, 0)?;
            let r = AppKey::new(&authority, &app).map(|key| it.svc.registry.borrow().list(&key));
            Ok(to_result(r.map_err(plugin_error), |v| {
                Value::List(v.into_iter().map(Value::Plugin).collect())
            }))
        }
        "update" => {
            argc(it, &full, &args, 2)?;
            let id = uuid_arg(it, &full, &args, 0)?;
            let p = plugin_arg(it, &full, &args, 1)?;
            let r = owned_by_page(it, id, &authority).and_then(|_| {
                it.svc
                    .registry
                    .borrow_mut()
                    .update(id, p, &it.svc.url, &*it.svc.loader)
            });
            Ok(to_result(r.map_err(plugin_error), |_| Value::Unit))
        }
        "delete" => {
            argc(it, &full, &args, 1)?;
            let id = uuid_arg(it, &full, &args, 0)?;
            let r = owned_by_page(it, id, &authority)
                .and_then(|_| it.svc.registry.borrow_mut().delete(id));
            Ok(to_result(r.map_err(plugin_error), |_| Value::Unit))
        }
        "clear" => {
            argc(it, &full, &args, 1)?;
            let app = str_arg(it, &full, &args, 0)?;
            let r = AppKey::new(&authority, &app)
                .and_then(|key| it.svc.registry.borrow_mut().clear(&key));
            Ok(to_result(r.map_err(plugin_error), |_| Value::Unit))
        }
        _ => Err(it.rt(format!("`root.plugin` has no function `{}`", name))),
    }
}

/// Plugins of other authorities are invisible: they report "not found".
fn owned_by_page(it: &Interpreter, id: Uuid, authority: &str) -> Result<(), PluginError> {
    match it.svc.registry.borrow().get(id) {
        Some(e) if e.key.authority() == authority => Ok(()),
        _ => Err(PluginError::NotFound(id)),
    }
}
