use super::{
    error::Error,
    value::{Constant, Value},
};
use crate::{card::Card, id::ObjectId, player::Player};
use bincode::{Decode, Encode};
use jaq_core::{
    load::{
        lex::StrPart,
        parse::{BinaryOp, Term},
    },
    ops::{Cmp, Math},
    path::{Opt, Part},
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashMap},
    str::FromStr,
};

const EXECUTION_LIMIT: usize = 256;

#[derive(Debug, Default, Clone, PartialEq, Encode, Decode)]
pub enum Exp {
    #[default]
    Ident,
    Path(Box<Self>, Vec<Path>),
    Variable(String),
    Value(Value),
    Arr(Option<Box<Self>>),
    Obj(Vec<(Box<Self>, Option<Box<Self>>)>),
    Assign(String, Box<Self>),
    Pipe(Box<Self>, Box<Self>),
    Comma(Box<Self>, Box<Self>),
    TryCatch(Box<Self>, Box<Self>),
    IfThenElse(Vec<(Self, Self)>, Option<Box<Self>>),
    BinOp(Box<Self>, BinOp, Box<Self>),
    Neg(Box<Self>),
    Str(Vec<Self>),
    Select(Box<Self>),
    Error(Box<Self>),
    CustomFunction(String, Vec<Self>),
    Not,
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Ge,
    Gt,
    Le,
    Lt,
}

impl FromStr for Exp {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let term = jaq_core::load::parse(s, |p| p.term()).ok_or(Error::InvalidSyntax)?;
        (&term).try_into()
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub enum Path {
    Index(Box<Exp>, bool),
    Range(Option<Box<Exp>>, Option<Box<Exp>>, bool),
}

#[derive(Debug)]
enum LiteralPath {
    Str(String, bool),
    Num(i64, bool),
    Range(Option<i64>, Option<i64>, bool),
}

impl<'a> TryFrom<&'a (Part<Term<&'a str>>, Opt)> for Path {
    type Error = Error;

    fn try_from(
        (part, opt): &'a (Part<Term<&'a str>>, Opt),
    ) -> Result<Self, <Self as TryFrom<&'a (Part<Term<&'a str>>, Opt)>>::Error> {
        match part {
            Part::Index(exp) => Ok(Self::Index(
                Box::new(Exp::try_from(exp)?),
                matches!(opt, Opt::Optional),
            )),
            Part::Range(lhs, rhs) => Ok(Self::Range(
                if let Some(lhs) = lhs {
                    Some(Box::new(Exp::try_from(lhs)?))
                } else {
                    None
                },
                if let Some(rhs) = rhs {
                    Some(Box::new(Exp::try_from(rhs)?))
                } else {
                    None
                },
                matches!(opt, Opt::Optional),
            )),
        }
    }
}

impl<'a> TryFrom<&'a Term<&'a str>> for Exp {
    type Error = Error;

    fn try_from(
        term: &'a Term<&'a str>,
    ) -> Result<Self, <Self as TryFrom<&'a Term<&'a str>>>::Error> {
        match term {
            Term::Id => Ok(Self::Ident),
            Term::Path(lhs, parts) => {
                let lhs = Self::try_from(&**lhs)?;
                let parts = parts
                    .0
                    .iter()
                    .map(Path::try_from)
                    .collect::<Result<_, _>>()?;
                Ok(Self::Path(Box::new(lhs), parts))
            }
            Term::Var(s) => Ok(Self::Variable(s.to_string())),
            Term::BinOp(lhs, op, rhs) => match (lhs.as_ref(), op, rhs.as_ref()) {
                (Term::Var(s), BinaryOp::Assign, rhs) => {
                    Ok(Self::Assign(s.to_string(), Box::new(Self::try_from(rhs)?)))
                }
                (lhs, BinaryOp::Comma, rhs) => Ok(Self::Comma(
                    Box::new(Self::try_from(lhs)?),
                    Box::new(Self::try_from(rhs)?),
                )),
                (lhs, op, rhs) => Ok(Self::BinOp(
                    Box::new(Self::try_from(lhs)?),
                    match op {
                        BinaryOp::Math(Math::Add) => BinOp::Add,
                        BinaryOp::Math(Math::Sub) => BinOp::Sub,
                        BinaryOp::Math(Math::Mul) => BinOp::Mul,
                        BinaryOp::Math(Math::Div) => BinOp::Div,
                        BinaryOp::Math(Math::Rem) => BinOp::Rem,
                        BinaryOp::Cmp(Cmp::Eq) => BinOp::Eq,
                        BinaryOp::Cmp(Cmp::Ne) => BinOp::Ne,
                        BinaryOp::Cmp(Cmp::Ge) => BinOp::Ge,
                        BinaryOp::Cmp(Cmp::Gt) => BinOp::Gt,
                        BinaryOp::Cmp(Cmp::Le) => BinOp::Le,
                        BinaryOp::Cmp(Cmp::Lt) => BinOp::Lt,
                        _ => return Err(Error::InvalidSyntax),
                    },
                    Box::new(Self::try_from(rhs)?),
                )),
            },
            Term::Neg(exp) => Ok(Self::Neg(Box::new(Self::try_from(&**exp)?))),
            Term::Pipe(lhs, None, rhs) => {
                let lhs = Self::try_from(&**lhs)?;
                let rhs = Self::try_from(&**rhs)?;
                Ok(Self::Pipe(Box::new(lhs), Box::new(rhs)))
            }
            Term::Num(n) => Ok(Self::Value(if let Ok(n) = n.parse::<u64>() {
                Value::Constant(Constant::U64(n))
            } else if let Ok(n) = n.parse::<i64>() {
                Value::Constant(Constant::I64(n))
            } else if let Ok(n) = n.parse::<f64>() {
                Value::Constant(Constant::F64(n))
            } else {
                return Err(Error::InvalidSyntax);
            })),
            Term::TryCatch(lhs, rhs) => Ok(Self::TryCatch(
                Box::new(Self::try_from(&**lhs)?),
                Box::new(
                    rhs.as_ref()
                        .map(|rhs| Self::try_from(&**rhs))
                        .unwrap_or(Ok(Self::Empty))?,
                ),
            )),
            Term::IfThenElse(ifthen, els) => {
                let ifthen = ifthen
                    .iter()
                    .map(|(lhs, rhs)| Ok((Self::try_from(lhs)?, Self::try_from(rhs)?)))
                    .collect::<Result<_, _>>()?;
                let els = if let Some(els) = els {
                    Some(Box::new(Self::try_from(&**els)?))
                } else {
                    None
                };
                Ok(Self::IfThenElse(ifthen, els))
            }
            Term::Str(None, parts) => {
                let mut args = vec![];
                for item in parts {
                    match item {
                        StrPart::Char(c) => {
                            args.push(Self::Value(Value::Constant(Constant::String(
                                c.to_string(),
                            ))));
                        }
                        StrPart::Str(s) => {
                            args.push(Self::Value(Value::Constant(Constant::String(
                                s.to_string(),
                            ))));
                        }
                        StrPart::Term(t) => {
                            args.push(Self::try_from(t)?);
                        }
                    }
                }
                Ok(Self::Str(args))
            }
            Term::Arr(arr) => {
                if let Some(arr) = arr {
                    Ok(Self::Arr(Some(Box::new(Self::try_from(&**arr)?))))
                } else {
                    Ok(Self::Arr(None))
                }
            }
            Term::Obj(pairs) => {
                let mut obj = vec![];
                for (lhs, rhs) in pairs {
                    obj.push((
                        Box::new(Self::try_from(lhs)?),
                        if let Some(rhs) = rhs {
                            Some(Box::new(Self::try_from(rhs)?))
                        } else {
                            None
                        },
                    ));
                }
                Ok(Self::Obj(obj))
            }
            Term::Call("null", _) => Ok(Self::Value(Value::Constant(Constant::Null))),
            Term::Call("empty", _) => Ok(Self::Empty),
            Term::Call("not", _) => Ok(Self::Not),
            Term::Call("nan", _) => Ok(Self::Value(Value::Constant(Constant::F64(f64::NAN)))),
            Term::Call("infinite", _) => {
                Ok(Self::Value(Value::Constant(Constant::F64(f64::INFINITY))))
            }
            Term::Call("true", _) => Ok(Self::Value(Value::Constant(Constant::Bool(true)))),
            Term::Call("false", _) => Ok(Self::Value(Value::Constant(Constant::Bool(false)))),
            Term::Call("error", msg) => {
                if let Some(msg) = msg.first() {
                    Ok(Self::Error(Box::new(Self::try_from(msg)?)))
                } else {
                    Err(Error::InvalidSyntax)
                }
            }
            Term::Call("select", exp) => {
                if let Some(exp) = exp.first() {
                    Ok(Self::Select(Box::new(Self::try_from(exp)?)))
                } else {
                    Err(Error::InvalidSyntax)
                }
            }
            Term::Call(name, args) => {
                let args = args.iter().map(Self::try_from).collect::<Result<_, _>>()?;
                Ok(Self::CustomFunction(name.to_string(), args))
            }
            _ => Err(Error::InvalidSyntax),
        }
    }
}

impl<'de> Deserialize<'de> for Exp {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl<'a> ExpExt<'a, &'a [Value]> for Exp {
    fn eval<T>(&self, ctx: &mut ExpContext<'a, T, &'a [Value]>) -> Result<Vec<Value>, Error>
    where
        T: ExpEnv,
    {
        let mut results = vec![];
        for item in ctx.input.iter() {
            let mut new_ctx = ExpContext {
                env: ctx.env,
                input: item,
                params: ctx.params,
            };
            let val = self.eval(&mut new_ctx)?;
            results.extend(val);
        }
        Ok(results)
    }
}

impl<'a> ExpExt<'a, &'a Value> for Exp {
    fn eval<T>(&self, ctx: &mut ExpContext<'a, T, &'a Value>) -> Result<Vec<Value>, Error>
    where
        T: ExpEnv,
    {
        ctx.params.consume_exec(1)?;

        match self {
            Self::Ident => Ok(vec![ctx.input.clone()]),
            Self::Path(lhs, parts) => {
                let mut new_ctx = ExpContext {
                    env: ctx.env,
                    input: ctx.input,
                    params: ctx.params,
                };
                let mut val = lhs.eval(&mut new_ctx)?;
                for part in parts {
                    let mut new_val = vec![];
                    let mut indices = vec![];
                    match part {
                        Path::Index(index, opt) => {
                            for i in index.eval(&mut new_ctx)? {
                                if let Value::Constant(n) = i {
                                    if let Constant::String(s) = n {
                                        indices.push(LiteralPath::Str(s, *opt));
                                    } else if let Some(i) = n.as_i64() {
                                        indices.push(LiteralPath::Num(i, *opt));
                                    } else {
                                        return Err(Error::InvalidKey);
                                    }
                                } else {
                                    return Err(Error::InvalidKey);
                                }
                            }
                        }
                        Path::Range(start, end, opt) => {
                            let mut start = if let Some(start) = start {
                                let mut indices = vec![];
                                for i in start.eval(&mut new_ctx)? {
                                    if let Value::Constant(n) = i {
                                        if let Some(i) = n.as_i64() {
                                            indices.push(Some(i));
                                        } else {
                                            return Err(Error::InvalidKey);
                                        }
                                    } else {
                                        return Err(Error::InvalidKey);
                                    }
                                }
                                indices
                            } else {
                                vec![]
                            };
                            let mut end = if let Some(end) = end {
                                let mut indices = vec![];
                                for i in end.eval(&mut new_ctx)? {
                                    if let Value::Constant(n) = i {
                                        if let Some(i) = n.as_i64() {
                                            indices.push(Some(i));
                                        } else {
                                            return Err(Error::InvalidKey);
                                        }
                                    } else {
                                        return Err(Error::InvalidKey);
                                    }
                                }
                                indices
                            } else {
                                vec![]
                            };
                            let len = start.len().max(end.len()).max(1);
                            start.resize_with(len, Default::default);
                            end.resize_with(len, Default::default);
                            for (start, end) in start.into_iter().zip(end) {
                                indices.push(LiteralPath::Range(start, end, *opt));
                            }
                        }
                    }

                    for i in &indices {
                        for v in &val {
                            if matches!(i, LiteralPath::Range(None, None, _)) {
                                match v {
                                    Value::Array(arr) => {
                                        new_val.extend(arr.iter().cloned());
                                        continue;
                                    }
                                    Value::Object(obj) => {
                                        new_val.extend(obj.values().cloned());
                                        continue;
                                    }
                                    _ => {
                                        return Err(Error::InvalidKey);
                                    }
                                }
                            }
                            let (result, opt) = match i {
                                LiteralPath::Str(s, opt) => (v.index_str(s, ctx.env), opt),
                                LiteralPath::Num(n, opt) => (v.index_num(*n), opt),
                                LiteralPath::Range(start, end, opt) => {
                                    (v.index_range(*start, *end), opt)
                                }
                            };

                            match result {
                                Ok(result) => {
                                    new_val.push(result);
                                }
                                Err(err) => {
                                    if !*opt {
                                        return Err(err);
                                    }
                                }
                            }
                        }
                    }
                    val = new_val;
                }
                Ok(val)
            }
            Self::Variable(name) => {
                let val = ctx
                    .env
                    .get_var(name)
                    .or_else(|| ctx.params.get_var(name))
                    .ok_or(Error::UndefinedVariable)?;
                Ok(vec![val])
            }
            Self::Value(n) => Ok(vec![n.clone()]),
            Self::Arr(exp) => {
                if let Some(exp) = exp {
                    Ok(vec![Value::Array(exp.eval(ctx)?)])
                } else {
                    Ok(vec![Value::Array(vec![])])
                }
            }
            Self::Obj(pairs) => {
                let mut obj = BTreeMap::new();
                for (lhs, rhs) in pairs {
                    for key in lhs.eval(ctx)? {
                        let key = key.to_string();
                        let mut val = if let Some(rhs) = rhs {
                            rhs.eval(ctx)?
                        } else {
                            vec![ctx.input.index_str(&key, ctx.env)?]
                        };
                        if let Some(last) = val.pop() {
                            obj.insert(key, last);
                        }
                    }
                }
                Ok(vec![Value::Object(obj)])
            }
            Self::Assign(name, exp) => {
                let val = exp.eval(ctx)?;
                if let Some(last) = val.last() {
                    ctx.params.set_var(name, last.clone());
                }
                Ok(val)
            }
            Self::BinOp(lhs, op, rhs) => {
                let lhs = lhs.eval(ctx)?;
                let rhs = rhs.eval(ctx)?;
                let mut results = vec![];
                for l in lhs {
                    for r in &rhs {
                        if let (
                            Value::Constant(Constant::String(_)),
                            Value::Constant(Constant::U64(n)),
                        ) = (&l, r)
                        {
                            if *op == BinOp::Mul {
                                ctx.params.consume_exec(*n as usize)?;
                            }
                        }
                        let val = match op {
                            BinOp::Add => (l.clone() + r.clone())?,
                            BinOp::Sub => (l.clone() - r.clone())?,
                            BinOp::Mul => (l.clone() * r.clone())?,
                            BinOp::Div => (l.clone() / r.clone())?,
                            BinOp::Rem => (l.clone() % r.clone())?,
                            BinOp::Eq => (l == *r).into(),
                            BinOp::Ne => (l != *r).into(),
                            BinOp::Ge => (l >= *r).into(),
                            BinOp::Gt => (l > *r).into(),
                            BinOp::Le => (l <= *r).into(),
                            BinOp::Lt => (l < *r).into(),
                        };
                        results.push(val);
                    }
                }
                Ok(results)
            }
            Self::Neg(exp) => {
                let val = exp.eval(ctx)?;
                let mut results = vec![];
                for v in val {
                    results.push((-v)?);
                }
                Ok(results)
            }
            Self::Pipe(lhs, rhs) => {
                let mut new_ctx = ExpContext {
                    env: ctx.env,
                    input: ctx.input,
                    params: ctx.params,
                };
                let val = lhs.eval(&mut new_ctx)?;
                let mut new_ctx = ExpContext {
                    env: ctx.env,
                    input: val.as_slice(),
                    params: ctx.params,
                };
                rhs.eval(&mut new_ctx)
            }
            Self::Comma(lhs, rhs) => {
                let mut new_ctx = ExpContext {
                    env: ctx.env,
                    input: ctx.input,
                    params: ctx.params,
                };
                let lhs_val = lhs.eval(&mut new_ctx)?;
                let rhs_val = rhs.eval(&mut new_ctx)?;
                Ok(lhs_val.iter().cloned().chain(rhs_val).collect())
            }
            Self::Str(args) => {
                let mut s = String::new();
                for arg in args {
                    let mut new_ctx = ExpContext {
                        env: ctx.env,
                        input: ctx.input,
                        params: ctx.params,
                    };
                    for v in arg.eval(&mut new_ctx)? {
                        s.push_str(&v.to_string());
                    }
                }
                Ok(vec![s.into()])
            }
            Self::Select(arg) => {
                let mut new_ctx = ExpContext {
                    env: ctx.env,
                    input: ctx.input,
                    params: ctx.params,
                };
                Ok(arg
                    .eval(&mut new_ctx)?
                    .into_iter()
                    .filter(|v| !!v)
                    .map(|_| ctx.input.clone())
                    .collect())
            }
            Self::TryCatch(lhs, rhs) => {
                let mut new_ctx = ExpContext {
                    env: ctx.env,
                    input: ctx.input,
                    params: ctx.params,
                };
                let val = lhs.eval(&mut new_ctx);
                match val {
                    Ok(val) => Ok(val),
                    Err(_) => {
                        let mut new_ctx = ExpContext {
                            env: ctx.env,
                            input: ctx.input,
                            params: ctx.params,
                        };
                        rhs.eval(&mut new_ctx)
                    }
                }
            }
            Self::IfThenElse(ifthen, els) => {
                for (cond, body) in ifthen {
                    let mut new_ctx = ExpContext {
                        env: ctx.env,
                        input: ctx.input,
                        params: ctx.params,
                    };
                    let val = cond.eval(&mut new_ctx)?;
                    if val.iter().any(|v| !!v) {
                        let mut new_ctx = ExpContext {
                            env: ctx.env,
                            input: ctx.input,
                            params: ctx.params,
                        };
                        return body.eval(&mut new_ctx);
                    }
                }
                if let Some(els) = els {
                    let mut new_ctx = ExpContext {
                        env: ctx.env,
                        input: ctx.input,
                        params: ctx.params,
                    };
                    els.eval(&mut new_ctx)
                } else {
                    Ok(vec![ctx.input.clone()])
                }
            }
            Self::Error(exp) => {
                let mut new_ctx = ExpContext {
                    env: ctx.env,
                    input: ctx.input,
                    params: ctx.params,
                };
                if let Some(msg) = exp.eval(&mut new_ctx)?.first() {
                    return Err(Error::Custom(msg.to_string()));
                }
                Ok(vec![])
            }
            Self::CustomFunction(name, args) => {
                if let Some(Exp::Value(Value::Function(func))) =
                    ctx.params.get_def(name, args.len())
                {
                    let func = func.clone();
                    let mut new_args = vec![];
                    for (name, exp) in func.args.iter().zip(args) {
                        if name.starts_with('$') {
                            let mut val = exp.eval(ctx)?;
                            if let Some(last) = val.pop() {
                                new_args.push(last);
                            }
                        } else {
                            let func = Function {
                                name: "".to_string(),
                                args: vec![],
                                body: exp.clone(),
                            };
                            new_args.push(Value::Function(Box::new(func)));
                        }
                    }
                    func.invoke(ctx, new_args)
                } else {
                    let mut new_ctx = ExpContext {
                        env: ctx.env,
                        input: ctx.input,
                        params: ctx.params,
                    };
                    let mut new_args = vec![];
                    for arg in args {
                        new_args.extend(arg.eval(&mut new_ctx)?);
                    }
                    ctx.env.invoke(name, new_args)
                }
            }
            Self::Empty => Ok(vec![]),
            Self::Not => Ok(vec![(!ctx.input).into()]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Function {
    pub name: String,
    pub args: Vec<String>,
    pub body: Exp,
}

impl Function {
    pub fn invoke<'a, T>(
        &self,
        ctx: &mut ExpContext<'a, T, &'a Value>,
        args: Vec<Value>,
    ) -> Result<Vec<Value>, Error>
    where
        T: ExpEnv,
    {
        ctx.params.push_vars();
        for (name, val) in self.args.iter().zip(args) {
            ctx.params.set_var(name, val);
        }
        let val = self.body.eval(ctx);
        ctx.params.pop_vars();
        val
    }
}

pub trait ExpExt<'a, I> {
    fn eval<T>(&self, ctx: &mut ExpContext<'a, T, I>) -> Result<Vec<Value>, Error>
    where
        T: ExpEnv;
}

pub trait ExpEnv {
    fn get_var(&self, name: &str) -> Option<Value>;
    fn get_card(&self, id: ObjectId) -> Option<&Card>;
    fn get_player(&self, id: u8) -> Option<&Player>;
    fn invoke(&self, name: &str, args: Vec<Value>) -> Result<Vec<Value>, Error>;
}

#[derive(Debug, Clone)]
pub struct ExpParams {
    pub vars: Vec<HashMap<String, Exp>>,
    pub execution_limit: usize,
}

impl ExpParams {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn consume_exec(&mut self, n: usize) -> Result<(), Error> {
        if self.execution_limit >= n {
            self.execution_limit -= n;
            Ok(())
        } else {
            Err(Error::ExecutionLimitExceeded)
        }
    }

    pub fn reset_exec(&mut self) {
        self.execution_limit = EXECUTION_LIMIT;
    }

    pub fn push_vars(&mut self) {
        self.vars.push(HashMap::new());
    }

    pub fn pop_vars(&mut self) {
        if self.vars.len() > 1 {
            self.vars.pop();
        }
    }

    pub fn get_var(&self, name: &str) -> Option<Value> {
        for vars in self.vars.iter().rev() {
            if let Some(Exp::Value(val)) = vars.get(name) {
                return Some(val.clone());
            }
        }
        None
    }

    pub fn set_var(&mut self, name: &str, val: Value) {
        if let Some(vars) = self.vars.last_mut() {
            vars.insert(name.to_string(), Exp::Value(val));
        }
    }

    pub fn get_def(&self, name: &str, arity: usize) -> Option<&Exp> {
        let id = if arity > 0 {
            format!("{name}/{arity}")
        } else {
            name.to_string()
        };
        for vars in self.vars.iter().rev() {
            if let Some(val) = vars.get(&id) {
                return Some(val);
            }
        }
        None
    }

    pub fn set_def(&mut self, name: &str, arity: usize, val: Exp) {
        let id = if arity > 0 {
            format!("{name}/{arity}")
        } else {
            name.to_string()
        };
        if let Some(vars) = self.vars.last_mut() {
            vars.insert(id, val);
        }
    }
}

impl Default for ExpParams {
    fn default() -> Self {
        Self {
            vars: vec![HashMap::new()],
            execution_limit: EXECUTION_LIMIT,
        }
    }
}

pub struct ExpContext<'a, T, I> {
    env: &'a T,
    input: I,
    params: &'a mut ExpParams,
}

impl<'a, T, I> ExpContext<'a, T, I> {
    pub fn new(env: &'a T, input: I, params: &'a mut ExpParams) -> Self {
        Self { env, input, params }
    }
}

pub struct Module {
    pub funcs: HashMap<String, Function>,
}

impl FromStr for Module {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let defs = jaq_core::load::parse(s, |p| p.defs()).ok_or(Error::InvalidSyntax)?;
        let mut funcs = HashMap::new();
        for def in defs {
            let name = def.name.to_string();
            funcs.insert(
                name.to_string(),
                Function {
                    name: name.to_string(),
                    args: def.args.iter().map(|s| s.to_string()).collect(),
                    body: (&def.body).try_into()?,
                },
            );
        }
        Ok(Self { funcs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Card;

    struct TestEnv {}

    impl ExpEnv for TestEnv {
        fn get_var(&self, name: &str) -> Option<Value> {
            match name {
                "$x" => Some(42.into()),
                _ => None,
            }
        }

        fn get_card(&self, _id: ObjectId) -> Option<&Card> {
            None
        }

        fn get_player(&self, _id: u8) -> Option<&Player> {
            None
        }

        fn invoke(&self, name: &str, args: Vec<Value>) -> Result<Vec<Value>, Error> {
            if name == "test_builtin_func" {
                Ok(vec![args.get(1).cloned().unwrap_or_default()])
            } else {
                Err(Error::UndefinedFilter)
            }
        }
    }

    #[test]
    fn test_module() {
        let module = Module::from_str(
            r#"
                def foo(f): f*2;
                def bar(f): f|f;
                def baz($f): $f|$f;
                def foo2: .+100;
            "#,
        )
        .unwrap();

        let env = TestEnv {};
        let array: Vec<Value> = vec![1.into()];

        let mut params = ExpParams::new();
        for (name, func) in module.funcs {
            params.set_def(
                &name,
                func.args.len(),
                Exp::Value(Value::Function(Box::new(func))),
            );
        }

        let mut ctx = ExpContext::new(&env, array.as_slice(), &mut params);
        let exp = Exp::from_str("foo(.)").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![2.into()]));

        let exp = Exp::from_str("5|bar(.*2)").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![20.into()]));

        let exp = Exp::from_str("5|baz(.*2)").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![10.into()]));

        let exp = Exp::from_str("5|bar(foo2)").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![205.into()]));
    }

    #[test]
    fn test_exp() {
        let env = TestEnv {};
        let array: Vec<Value> = vec!["input".into(), 123.into()];

        let mut params = ExpParams::new();

        let func = Function {
            name: "foo".to_string(),
            args: vec!["$a".to_string(), "b".to_string()],
            body: Exp::from_str("[$a|$a, b|b]").unwrap(),
        };
        params.set_def(
            "foo",
            func.args.len(),
            Exp::Value(Value::Function(Box::new(func))),
        );

        let mut ctx = ExpContext::new(&env, array.as_slice(), &mut params);

        let exp = Exp::from_str(".").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(array.clone()));

        let exp = Exp::from_str("$x").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![42.into(), 42.into()]));

        let exp = Exp::from_str("$none").unwrap();
        assert_eq!(exp.eval(&mut ctx), Err(Error::UndefinedVariable));

        let exp = Exp::from_str("$new = $x").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![42.into(), 42.into()]));
        assert_eq!(ctx.params.get_var("$new"), Some(42.into()));

        let exp = Exp::from_str(". | $new").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![42.into(), 42.into()]));

        let exp = Exp::from_str("true").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![true.into(), true.into()]));

        let exp = Exp::from_str("false").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![false.into(), false.into()]));

        let exp = Exp::from_str("89.4").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![(89.4).into(), (89.4).into()]));

        let exp = Exp::from_str("89.4 - 25").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![(64.4).into(), (64.4).into()]));

        let exp = Exp::from_str("89 / 25").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![(3.56).into(), (3.56).into()]));

        let exp = Exp::from_str(". == 123").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![false.into(), true.into()]));

        let exp = Exp::from_str("89 != 89").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![false.into(), false.into()]));

        let exp = Exp::from_str("-89 < 89").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![true.into(), true.into()]));

        let exp = Exp::from_str(". >= 999").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![true.into(), false.into()]));

        let exp = Exp::from_str("5|foo(.*2; .*2)").unwrap();
        assert_eq!(
            exp.eval(&mut ctx),
            Ok(vec![
                Value::Array(vec![20.into(), 40.into()]),
                Value::Array(vec![20.into(), 40.into()])
            ])
        );

        let exp = Exp::from_str("[.]").unwrap();
        assert_eq!(
            exp.eval(&mut ctx),
            Ok(vec![
                Value::Array(vec!["input".into()]),
                Value::Array(vec![123.into()])
            ])
        );

        let exp = Exp::from_str("[.,.] | .[]").unwrap();
        assert_eq!(
            exp.eval(&mut ctx),
            Ok(vec!["input".into(), "input".into(), 123.into(), 123.into()])
        );

        let exp = Exp::from_str(".[0:1]?[0:1]").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec!["i".into()]));

        let exp = Exp::from_str(".[2:4]?").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec!["pu".into()]));

        let exp = Exp::from_str("{a: ., b: . * 2}").unwrap();
        assert_eq!(
            exp.eval(&mut ctx),
            Ok(vec![
                Value::Object(
                    vec![
                        ("a".into(), "input".into()),
                        ("b".into(), "inputinput".into())
                    ]
                    .into_iter()
                    .collect()
                ),
                Value::Object(
                    vec![("a".into(), 123.into()), ("b".into(), 246.into())]
                        .into_iter()
                        .collect()
                )
            ])
        );

        let exp = Exp::from_str("{a: 100} | .[]").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![100.into(), 100.into()]));

        let exp = Exp::from_str(". < \"bbb\"").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![false.into(), true.into()]));

        let exp = Exp::from_str("select(. * 0)").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec!["input".into(), 123.into()]));

        let exp = Exp::from_str("if . == \"input\" then \"output\" end").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec!["output".into(), 123.into()]));

        let exp =
            Exp::from_str("if . == \"input\" then \"output\" elif . > 100 then \"\" else 1 end")
                .unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec!["output".into(), "".into()]));

        let exp = Exp::from_str(". / .").unwrap();
        assert_eq!(
            exp.eval(&mut ctx),
            Ok(vec![Value::Array(vec!["".into(), "".into()]), 1.into()])
        );

        let exp = Exp::from_str("89 % 25").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![14.into(), 14.into()]));

        let exp = Exp::from_str("89.4, 55, . + .").unwrap();
        assert_eq!(
            exp.eval(&mut ctx),
            Ok(vec![
                (89.4).into(),
                55.into(),
                ("inputinput").into(),
                (89.4).into(),
                55.into(),
                246.into()
            ])
        );

        ctx.params.reset_exec();
        let exp = Exp::from_str("null").unwrap();
        assert_eq!(
            exp.eval(&mut ctx),
            Ok(vec![Value::default(), Value::default()])
        );

        let exp = Exp::from_str("empty").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![]));

        let exp = Exp::from_str("test_builtin_func(0, 1)").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![1.into(), 1.into()]));

        let exp = Exp::from_str("try (. - 5) catch 0").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![0.into(), 118.into()]));

        let exp = Exp::from_str(". * 3").unwrap();
        assert_eq!(
            exp.eval(&mut ctx),
            Ok(vec!["inputinputinput".into(), 369.into()])
        );

        let exp = Exp::from_str(". | not").unwrap();
        assert_eq!(exp.eval(&mut ctx), Ok(vec![false.into(), false.into()]));

        let exp = Exp::from_str("\"hello \\(.) hello\"").unwrap();
        assert_eq!(
            exp.eval(&mut ctx),
            Ok(vec!["hello input hello".into(), "hello 123 hello".into()])
        );

        let exp = Exp::from_str("error(\"hello\")").unwrap();
        assert_eq!(exp.eval(&mut ctx), Err(Error::Custom("hello".to_string())));

        ctx.params.reset_exec();
        let exp = Exp::from_str(". * 999999").unwrap();
        assert_eq!(exp.eval(&mut ctx), Err(Error::ExecutionLimitExceeded));

        ctx.params.reset_exec();
        let exp = Exp::from_str(".,.,.,.|.,.,.,.|.,.,.,.|.,.,.,.|.,.,.,.|.,.,.,.").unwrap();
        assert_eq!(exp.eval(&mut ctx), Err(Error::ExecutionLimitExceeded));
    }
}
