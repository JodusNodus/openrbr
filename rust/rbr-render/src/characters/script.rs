//! Runs the character scripts' Lua subset (`rbr_formats::lua`): the `load_*.lua` scripts that pick a model and clips,
//! and the `run_*.lua` state machines (`Update(state)` when a clip ends, `Trigger(state, isStart)` when the car comes
//! by; rbr-recomp `notes/stage-characters.md`, sections 2-5). Pure: the exe's functions (`LuaRandom`, `app:LoadCharacter`, ...)
//! are the caller's [`Host`]. Lua 4.0 semantics: no booleans, only `nil` is false, comparisons and `not` give 1 or
//! `nil`.

use rbr_formats::lua::{BinOp, Block, Chunk, Expr, Stmt};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Number(f64),
    Str(String),
}

impl Value {
    fn truthy(&self) -> bool {
        !matches!(self, Value::Nil)
    }

    pub fn number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            Value::Str(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<String> {
        match self {
            Value::Str(s) => Some(s.clone()),
            Value::Number(n) => Some(number_text(*n)),
            _ => None,
        }
    }
}

/// Lua 5.0's `%.14g` for the integers the scripts concatenate.
fn number_text(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// The functions a script calls that it does not define: `name(args)` comes as `name`, `object:method(args)` as
/// `object:method`.
pub trait Host {
    fn call(&mut self, function: &str, args: &[Value]) -> Result<Vec<Value>, String>;
}

#[derive(Debug, Clone)]
struct Function {
    params: Vec<String>,
    body: Block,
}

/// One Lua state: its globals and functions. The exe runs every character's scripts in one state, so a script's
/// globals stay until another assigns them.
#[derive(Debug, Clone, Default)]
pub struct Lua {
    globals: HashMap<String, Value>,
    functions: HashMap<String, Function>,
}

/// Calls nest no deeper than this (the retail scripts never call their own functions).
const MAX_DEPTH: usize = 32;

enum Flow {
    Normal,
    Return(Vec<Value>),
}

impl Lua {
    pub fn new() -> Lua {
        Lua::default()
    }

    pub fn set_global(&mut self, name: &str, value: Value) {
        self.globals.insert(name.to_owned(), value);
    }

    pub fn global(&self, name: &str) -> Option<&Value> {
        self.globals.get(name)
    }

    /// `lua_dostring`: runs the chunk's top-level statements.
    pub fn run(&mut self, chunk: &Chunk, host: &mut dyn Host) -> Result<Vec<Value>, String> {
        let mut scopes = vec![HashMap::new()];
        match self.block(&chunk.body, &mut scopes, host, 0)? {
            Flow::Return(values) => Ok(values),
            Flow::Normal => Ok(Vec::new()),
        }
    }

    /// Calls the script function `name`, or `None` when no script defined it.
    pub fn call(&mut self, name: &str, args: &[Value], host: &mut dyn Host) -> Option<Result<Vec<Value>, String>> {
        self.functions.contains_key(name).then(|| self.call_at(name, args, host, 0))
    }

    fn call_at(&mut self, name: &str, args: &[Value], host: &mut dyn Host, depth: usize) -> Result<Vec<Value>, String> {
        if depth > MAX_DEPTH {
            return Err(format!("{name}: calls nest too deep"));
        }
        let function = self.functions.get(name).cloned().ok_or_else(|| format!("no function {name}"))?;
        let mut scope = HashMap::new();
        for (i, param) in function.params.iter().enumerate() {
            scope.insert(param.clone(), args.get(i).cloned().unwrap_or(Value::Nil));
        }
        let mut scopes = vec![scope];
        match self.block(&function.body, &mut scopes, host, depth + 1)? {
            Flow::Return(values) => Ok(values),
            Flow::Normal => Ok(Vec::new()),
        }
    }

    fn block(
        &mut self,
        block: &Block,
        scopes: &mut Vec<HashMap<String, Value>>,
        host: &mut dyn Host,
        depth: usize,
    ) -> Result<Flow, String> {
        for stmt in block {
            match stmt {
                Stmt::Function { name, params, body } => {
                    self.functions.insert(name.clone(), Function { params: params.clone(), body: body.clone() });
                }
                Stmt::Assign { local, names, values } => {
                    let values = self.exprs(values, scopes, host, depth)?;
                    for (i, name) in names.iter().enumerate() {
                        let value = values.get(i).cloned().unwrap_or(Value::Nil);
                        if *local {
                            scopes.last_mut().expect("a scope").insert(name.clone(), value);
                        } else if let Some(scope) = scopes.iter_mut().rev().find(|s| s.contains_key(name)) {
                            scope.insert(name.clone(), value);
                        } else {
                            self.globals.insert(name.clone(), value);
                        }
                    }
                }
                Stmt::Call(call) => {
                    self.multi(call, scopes, host, depth)?;
                }
                Stmt::If { branches, otherwise } => {
                    let mut chosen = Some(otherwise);
                    for (condition, body) in branches {
                        if self.expr(condition, scopes, host, depth)?.truthy() {
                            chosen = Some(body);
                            break;
                        }
                    }
                    if let Some(body) = chosen {
                        scopes.push(HashMap::new());
                        let flow = self.block(body, scopes, host, depth);
                        scopes.pop();
                        if let Flow::Return(values) = flow? {
                            return Ok(Flow::Return(values));
                        }
                    }
                }
                Stmt::Return(values) => return Ok(Flow::Return(self.exprs(values, scopes, host, depth)?)),
            }
        }
        Ok(Flow::Normal)
    }

    /// An expression list: every value but the last is truncated to one, the last call expands (Lua's rule).
    fn exprs(
        &mut self,
        exprs: &[Expr],
        scopes: &mut Vec<HashMap<String, Value>>,
        host: &mut dyn Host,
        depth: usize,
    ) -> Result<Vec<Value>, String> {
        let mut out = Vec::with_capacity(exprs.len());
        for (i, expr) in exprs.iter().enumerate() {
            if i + 1 == exprs.len() {
                out.extend(self.multi(expr, scopes, host, depth)?);
            } else {
                out.push(self.expr(expr, scopes, host, depth)?);
            }
        }
        Ok(out)
    }

    fn multi(
        &mut self,
        expr: &Expr,
        scopes: &mut Vec<HashMap<String, Value>>,
        host: &mut dyn Host,
        depth: usize,
    ) -> Result<Vec<Value>, String> {
        match expr {
            Expr::Call { function, args } => {
                let args = self.exprs(args, scopes, host, depth)?;
                if self.functions.contains_key(function) {
                    self.call_at(function, &args, host, depth)
                } else {
                    host.call(function, &args)
                }
            }
            Expr::Method { object, method, args } => {
                let args = self.exprs(args, scopes, host, depth)?;
                host.call(&format!("{object}:{method}"), &args)
            }
            other => Ok(vec![self.expr(other, scopes, host, depth)?]),
        }
    }

    fn expr(
        &mut self,
        expr: &Expr,
        scopes: &mut Vec<HashMap<String, Value>>,
        host: &mut dyn Host,
        depth: usize,
    ) -> Result<Value, String> {
        Ok(match expr {
            Expr::Nil => Value::Nil,
            Expr::Number(n) => Value::Number(*n),
            Expr::Str(s) => Value::Str(s.clone()),
            Expr::Name(name) => scopes
                .iter()
                .rev()
                .find_map(|s| s.get(name))
                .or_else(|| self.globals.get(name))
                .cloned()
                .unwrap_or(Value::Nil),
            Expr::Call { .. } | Expr::Method { .. } => {
                self.multi(expr, scopes, host, depth)?.into_iter().next().unwrap_or(Value::Nil)
            }
            Expr::Not(inner) => truth(!self.expr(inner, scopes, host, depth)?.truthy()),
            Expr::Negate(inner) => {
                let v = self.expr(inner, scopes, host, depth)?;
                Value::Number(-v.number().ok_or("negating a non-number")?)
            }
            Expr::Binary { op: BinOp::And, left, right } => {
                let l = self.expr(left, scopes, host, depth)?;
                if l.truthy() {
                    self.expr(right, scopes, host, depth)?
                } else {
                    l
                }
            }
            Expr::Binary { op: BinOp::Or, left, right } => {
                let l = self.expr(left, scopes, host, depth)?;
                if l.truthy() {
                    l
                } else {
                    self.expr(right, scopes, host, depth)?
                }
            }
            Expr::Binary { op, left, right } => {
                let l = self.expr(left, scopes, host, depth)?;
                let r = self.expr(right, scopes, host, depth)?;
                binary(*op, &l, &r)?
            }
        })
    }
}

/// Lua 4.0's boolean result: 1 or `nil`.
fn truth(b: bool) -> Value {
    if b {
        Value::Number(1.0)
    } else {
        Value::Nil
    }
}

fn binary(op: BinOp, l: &Value, r: &Value) -> Result<Value, String> {
    let numbers = || match (l.number(), r.number()) {
        (Some(a), Some(b)) => Ok((a, b)),
        _ => Err(format!("{op:?} on {l:?} and {r:?}")),
    };
    let order = || match (l, r) {
        (Value::Number(a), Value::Number(b)) => a.partial_cmp(b).ok_or_else(|| "NaN compared".to_owned()),
        (Value::Str(a), Value::Str(b)) => Ok(a.cmp(b)),
        _ => Err(format!("{op:?} on {l:?} and {r:?}")),
    };
    use std::cmp::Ordering::*;
    Ok(match op {
        BinOp::Eq => truth(l == r),
        BinOp::Ne => truth(l != r),
        BinOp::Lt => truth(order()? == Less),
        BinOp::Le => truth(order()? != Greater),
        BinOp::Gt => truth(order()? == Greater),
        BinOp::Ge => truth(order()? != Less),
        BinOp::Concat => match (l.text(), r.text()) {
            (Some(a), Some(b)) => Value::Str(a + &b),
            _ => return Err(format!("concatenating {l:?} and {r:?}")),
        },
        BinOp::Add => numbers().map(|(a, b)| Value::Number(a + b))?,
        BinOp::Sub => numbers().map(|(a, b)| Value::Number(a - b))?,
        BinOp::Mul => numbers().map(|(a, b)| Value::Number(a * b))?,
        BinOp::Div => numbers().map(|(a, b)| Value::Number(a / b))?,
        BinOp::And | BinOp::Or => unreachable!("short-circuited above"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_formats::lua;

    /// Records host calls; `LuaRandom` returns its upper bound.
    #[derive(Default)]
    struct Recorder(Vec<(String, Vec<Value>)>);

    impl Host for Recorder {
        fn call(&mut self, function: &str, args: &[Value]) -> Result<Vec<Value>, String> {
            self.0.push((function.to_owned(), args.to_vec()));
            Ok(match function {
                "LuaRandom" => vec![args[1].clone()],
                _ => Vec::new(),
            })
        }
    }

    const LOAD: &[u8] = br#"
name = "media_left_flat_load"
scale = 0.001
function Load ( x )
    local randomValue = LuaRandom( 1, 3 )
    if( randomValue == 1 )
    then
        app:LoadAnimation( rootpath.."Sga\\a.sga", "start_idle" .. x )
    end
    if( randomValue == 3 )
    then
        app:LoadAnimation( rootpath.."Sga\\c.sga", "start_idle" .. x )
    end
    app:AddAnimationToCharacter( x, "start_idle" .. x )
    return 0
end
"#;

    const RUN: &[u8] = br#"
function Update( state )
    if state == 0
    then
        return 0, state
    end
    if state == 1
    then
        state = 2
        return 1, state
    end
    return 2, state
end
function Trigger( state, isStart )
    if state == 0
    then
        state = 1
    end
    return state
end
"#;

    #[test]
    fn load_script_picks_and_calls_the_host() {
        let mut lua = Lua::new();
        lua.set_global("rootpath", Value::Str(String::new()));
        let mut host = Recorder::default();
        lua.run(&lua::parse(LOAD).unwrap(), &mut host).unwrap();
        assert_eq!(lua.global("name"), Some(&Value::Str("media_left_flat_load".into())));
        assert_eq!(lua.global("scale").and_then(Value::number), Some(0.001));
        let result = lua.call("Load", &[Value::Str("m_7".into())], &mut host).unwrap().unwrap();
        assert_eq!(result, [Value::Number(0.0)]);
        let calls: Vec<String> = host.0.iter().map(|(f, a)| format!("{f}{a:?}")).collect();
        assert_eq!(
            calls,
            [
                "LuaRandom[Number(1.0), Number(3.0)]",
                r#"app:LoadAnimation[Str("Sga\\c.sga"), Str("start_idlem_7")]"#,
                r#"app:AddAnimationToCharacter[Str("m_7"), Str("start_idlem_7")]"#,
            ]
        );
    }

    #[test]
    fn run_script_state_machine() {
        let mut lua = Lua::new();
        let mut host = Recorder::default();
        lua.run(&lua::parse(RUN).unwrap(), &mut host).unwrap();
        let mut call = |name: &str, args: &[f64]| -> Vec<f64> {
            let args: Vec<Value> = args.iter().map(|&a| Value::Number(a)).collect();
            lua.call(name, &args, &mut host).unwrap().unwrap().iter().map(|v| v.number().unwrap()).collect()
        };
        assert_eq!(call("Update", &[0.0]), [0.0, 0.0]);
        assert_eq!(call("Trigger", &[0.0, 1.0]), [1.0]);
        assert_eq!(call("Update", &[1.0]), [1.0, 2.0]);
        assert_eq!(call("Update", &[2.0]), [2.0, 2.0]);
        assert_eq!(call("Trigger", &[2.0, 1.0]), [2.0]);
        assert!(lua.call("Missing", &[], &mut host).is_none());
    }

    #[test]
    fn operators() {
        let mut lua = Lua::new();
        let chunk = lua::parse(b"a = 7 / 2\nb = not nil and 1 or 2\nc = 1 .. \"x\"\nd = 3 >= 3 and 2 < 1\n").unwrap();
        lua.run(&chunk, &mut Recorder::default()).unwrap();
        assert_eq!(lua.global("a"), Some(&Value::Number(3.5)));
        assert_eq!(lua.global("b"), Some(&Value::Number(1.0)));
        assert_eq!(lua.global("c"), Some(&Value::Str("1x".into())));
        assert_eq!(lua.global("d"), Some(&Value::Nil));
    }
}
