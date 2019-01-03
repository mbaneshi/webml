use crate::config::Config;
use crate::lir::*;
use crate::mir;
use crate::pass::Pass;
use crate::prim::*;
use std::collections::HashMap;

pub struct MIR2LIR;

impl MIR2LIR {
    pub fn new() -> Self {
        MIR2LIR
    }

    fn ebbty_to_lty<'a>(&self, ty: &mir::EbbTy) -> LTy {
        use crate::mir::EbbTy::*;
        match *ty {
            Unit => LTy::Unit,
            Int => LTy::I32,
            Float => LTy::F64,
            Bool => LTy::I32,
            Tuple(_) => LTy::Ptr,
            Cls { .. } => LTy::Ptr,
            Ebb { .. } => LTy::FPtr,
        }
    }

    pub fn trans_mir(&self, mir: mir::MIR) -> LIR {
        LIR(mir.0.into_iter().map(|f| self.trans_function(f)).collect())
    }

    fn trans_function(&self, f: mir::Function) -> Function {
        use crate::lir::Op::*;
        use crate::lir::Value::*;
        use crate::mir::Op as m;
        let mir::Function {
            name,
            body,
            body_ty,
        } = f;
        let nparams = body[0].params.len() as u32;
        let ret_ty = self.ebbty_to_lty(&body_ty);
        let mut regs = Vec::new();
        let mut id = 0;
        let mut blocks = Vec::new();
        {
            // limit the scope of new_reg

            let mut new_reg = |ty| {
                let reg = Reg(ty, id);
                regs.push(reg.clone());
                id += 1;
                reg
            };

            let symbol_table = self.make_symbol_table(body.as_ref(), &mut new_reg);
            let target_table = self.make_target_table(body.as_ref(), &symbol_table);
            macro_rules! reg {
                ($var: expr) => {
                    symbol_table
                        .get(&$var)
                        .expect("variable resolution failed")
                        .clone()
                };
            }

            for ebb in body.iter() {
                let mut ops = Vec::new();
                for op in ebb.body.iter() {
                    match op {
                        &m::Lit {
                            ref var, ref value, ..
                        } => match value {
                            &Literal::Bool(b) => ops.push(ConstI32(reg!(var), b as u32)),
                            &Literal::Int(i) => ops.push(ConstI32(reg!(var), i as u32)),
                            &Literal::Float(f) => ops.push(ConstF64(reg!(var), f as f64)),
                        },
                        &m::Alias {
                            ref var,
                            ref ty,
                            ref sym,
                        } => {
                            match ty {
                                &mir::EbbTy::Unit => {
                                    // do nothing
                                    ()
                                }
                                &mir::EbbTy::Bool => ops.push(MoveI32(reg!(var), reg!(sym))),
                                &mir::EbbTy::Int
                                | &mir::EbbTy::Tuple(_)
                                | &mir::EbbTy::Cls { .. }
                                | &mir::EbbTy::Ebb { .. } => {
                                    ops.push(MoveI64(reg!(var), reg!(sym)))
                                }
                                &mir::EbbTy::Float => ops.push(MoveF64(reg!(var), reg!(sym))),
                            }
                        }
                        &m::Add {
                            ref var,
                            ref ty,
                            ref l,
                            ref r,
                        } => {
                            if ty == &mir::EbbTy::Int {
                                ops.push(AddI32(reg!(var), reg!(l), reg!(r)));
                            } else {
                                assert_eq!(ty, &mir::EbbTy::Float);
                                ops.push(AddF64(reg!(var), reg!(l), reg!(r)));
                            }
                        }
                        &m::Sub {
                            ref var,
                            ref ty,
                            ref l,
                            ref r,
                        } => {
                            if ty == &mir::EbbTy::Int {
                                ops.push(SubI32(reg!(var), reg!(l), reg!(r)));
                            } else {
                                assert_eq!(ty, &mir::EbbTy::Float);
                                ops.push(SubF64(reg!(var), reg!(l), reg!(r)));
                            }
                        }
                        &m::Mul {
                            ref var,
                            ref ty,
                            ref l,
                            ref r,
                        } => {
                            if ty == &mir::EbbTy::Int {
                                ops.push(MulI32(reg!(var), reg!(l), reg!(r)));
                            } else {
                                assert_eq!(ty, &mir::EbbTy::Float);
                                ops.push(MulF64(reg!(var), reg!(l), reg!(r)));
                            }
                        }
                        &m::DivInt {
                            ref var,
                            ref l,
                            ref r,
                            ..
                        } => {
                            ops.push(DivI32(reg!(var), reg!(l), reg!(r)));
                        }
                        &m::DivFloat {
                            ref var,
                            ref l,
                            ref r,
                            ..
                        } => {
                            ops.push(DivF64(reg!(var), reg!(l), reg!(r)));
                        }
                        &m::Mod {
                            ref var,
                            ref l,
                            ref r,
                            ..
                        } => {
                            ops.push(ModI32(reg!(var), reg!(l), reg!(r)));
                        }
                        &m::Eq {
                            ref var,
                            ref l,
                            ref r,
                            ..
                        } => match (&symbol_table[l].0, &symbol_table[r].0) {
                            (&LTy::I32, &LTy::I32) => ops.push(EqI32(reg!(var), reg!(l), reg!(r))),
                            (&LTy::I64, &LTy::I64) => ops.push(EqI64(reg!(var), reg!(l), reg!(r))),
                            (&LTy::F32, &LTy::F32) => ops.push(EqF32(reg!(var), reg!(l), reg!(r))),
                            (&LTy::F64, &LTy::F64) => ops.push(EqF64(reg!(var), reg!(l), reg!(r))),
                            ty => panic!("unknown overloaded ty {:?} for eq", ty),
                        },
                        &m::Neq {
                            ref var,
                            ref l,
                            ref r,
                            ..
                        } => match (&symbol_table[l].0, &symbol_table[r].0) {
                            (&LTy::I32, &LTy::I32) => ops.push(NeqI32(reg!(var), reg!(l), reg!(r))),
                            (&LTy::I64, &LTy::I64) => ops.push(NeqI64(reg!(var), reg!(l), reg!(r))),
                            (&LTy::F32, &LTy::F32) => ops.push(NeqF32(reg!(var), reg!(l), reg!(r))),
                            (&LTy::F64, &LTy::F64) => ops.push(NeqF64(reg!(var), reg!(l), reg!(r))),
                            ty => panic!("unknown overloaded ty {:?} for neq", ty),
                        },
                        &m::Gt {
                            ref var,
                            ref l,
                            ref r,
                            ..
                        } => match (&symbol_table[l].0, &symbol_table[r].0) {
                            (&LTy::I32, &LTy::I32) => ops.push(GtI32(reg!(var), reg!(l), reg!(r))),
                            (&LTy::I64, &LTy::I64) => ops.push(GtI64(reg!(var), reg!(l), reg!(r))),
                            (&LTy::F32, &LTy::F32) => ops.push(GtF32(reg!(var), reg!(l), reg!(r))),
                            (&LTy::F64, &LTy::F64) => ops.push(GtF64(reg!(var), reg!(l), reg!(r))),
                            ty => panic!("unknown overloaded ty {:?} for gt", ty),
                        },
                        &m::Ge {
                            ref var,
                            ref l,
                            ref r,
                            ..
                        } => match (&symbol_table[l].0, &symbol_table[r].0) {
                            (&LTy::I32, &LTy::I32) => ops.push(GeI32(reg!(var), reg!(l), reg!(r))),
                            (&LTy::I64, &LTy::I64) => ops.push(GeI64(reg!(var), reg!(l), reg!(r))),
                            (&LTy::F32, &LTy::F32) => ops.push(GeF32(reg!(var), reg!(l), reg!(r))),
                            (&LTy::F64, &LTy::F64) => ops.push(GeF64(reg!(var), reg!(l), reg!(r))),
                            ty => panic!("unknown overloaded ty {:?} for ge", ty),
                        },
                        &m::Lt {
                            ref var,
                            ref l,
                            ref r,
                            ..
                        } => match (&symbol_table[l].0, &symbol_table[r].0) {
                            (&LTy::I32, &LTy::I32) => ops.push(LtI32(reg!(var), reg!(l), reg!(r))),
                            (&LTy::I64, &LTy::I64) => ops.push(LtI64(reg!(var), reg!(l), reg!(r))),
                            (&LTy::F32, &LTy::F32) => ops.push(LtF32(reg!(var), reg!(l), reg!(r))),
                            (&LTy::F64, &LTy::F64) => ops.push(LtF64(reg!(var), reg!(l), reg!(r))),
                            ty => panic!("unknown overloaded ty {:?} for lt", ty),
                        },
                        &m::Le {
                            ref var,
                            ref l,
                            ref r,
                            ..
                        } => match (&symbol_table[l].0, &symbol_table[r].0) {
                            (&LTy::I32, &LTy::I32) => ops.push(LeI32(reg!(var), reg!(l), reg!(r))),
                            (&LTy::I64, &LTy::I64) => ops.push(LeI64(reg!(var), reg!(l), reg!(r))),
                            (&LTy::F32, &LTy::F32) => ops.push(LeF32(reg!(var), reg!(l), reg!(r))),
                            (&LTy::F64, &LTy::F64) => ops.push(LeF64(reg!(var), reg!(l), reg!(r))),
                            ty => panic!("unknown overloaded ty {:?} for le", ty),
                        },
                        &m::Tuple {
                            ref var,
                            ref tys,
                            ref tuple,
                        } => {
                            let reg = reg!(var);

                            let tys: Vec<_> = tys.iter().map(|ty| self.ebbty_to_lty(ty)).collect();
                            // currently all the items are aligned to 8
                            let size: u32 = tys.iter().map(|ty| 8).sum();
                            // let size: u32 = tys.iter().map(|ty| ty.size()).sum();

                            ops.push(HeapAlloc(reg.clone(), I(size as i32), tys.clone()));

                            let mut acc = 0;
                            for (var, ty) in tuple.iter().zip(tys) {
                                match ty {
                                    LTy::Unit => {
                                        // do nothing
                                        ()
                                    }
                                    LTy::I32 => {
                                        ops.push(StoreI32(Addr(reg.clone(), acc), reg!(var)))
                                    }
                                    LTy::I64 => {
                                        ops.push(StoreI64(Addr(reg.clone(), acc), reg!(var)))
                                    }
                                    LTy::F32 => {
                                        ops.push(StoreF32(Addr(reg.clone(), acc), reg!(var)))
                                    }
                                    LTy::F64 => {
                                        ops.push(StoreF64(Addr(reg.clone(), acc), reg!(var)))
                                    }
                                    LTy::Ptr => {
                                        ops.push(StoreI32(Addr(reg.clone(), acc), reg!(var)))
                                    }
                                    LTy::FPtr => {
                                        ops.push(StoreI32(Addr(reg.clone(), acc), reg!(var)))
                                    }
                                }
                                acc += 8;
                            }
                        }
                        &m::Proj {
                            ref var,
                            ref ty,
                            ref index,
                            ref tuple,
                        } => {
                            loop {
                                let ctor = match self.ebbty_to_lty(ty) {
                                    LTy::F32 => LoadF32,
                                    LTy::F64 => LoadF64,
                                    LTy::I32 => LoadI32,
                                    LTy::I64 => LoadI64,
                                    LTy::Ptr => LoadI64,
                                    LTy::FPtr => LoadI64,
                                    LTy::Unit =>
                                    // do nothing
                                    {
                                        break
                                    }
                                };
                                ops.push(ctor(reg!(var), Addr(reg!(tuple), *index * 8)));
                                break;
                            }
                        }
                        &m::Closure {
                            ref var,
                            ref fun,
                            ref env,
                            ..
                        } => {
                            // closure looks like on memory:
                            //   64      64    ...
                            // +-----------------------
                            // | fptr | arg1 | ...
                            // +-----------------------

                            let reg = reg!(var);
                            let mut size: u32 = LTy::Ptr.size();
                            size += env
                                .iter()
                                .map(|&(ref ty, _)| self.ebbty_to_lty(ty).size())
                                .sum::<u32>();
                            let mut tys = vec![LTy::FPtr];
                            for &(ref ty, _) in env.iter() {
                                tys.push(self.ebbty_to_lty(ty));
                            }
                            ops.push(HeapAlloc(reg.clone(), I(size as i32), tys));
                            // FIXME: explicitly take fun pointer
                            ops.push(StoreFnPtr(Addr(reg.clone(), 0), fun.clone()));
                            let mut acc = LTy::FPtr.size();
                            for &(ref ty, ref var) in env.iter() {
                                let ty = self.ebbty_to_lty(ty);
                                match ty {
                                    LTy::Unit => {
                                        // FIXME: remove unit from closure
                                        // do nothing
                                        ()
                                    }
                                    LTy::I32 => {
                                        ops.push(StoreI32(Addr(reg.clone(), acc), reg!(var)))
                                    }
                                    LTy::I64 => {
                                        ops.push(StoreI64(Addr(reg.clone(), acc), reg!(var)))
                                    }
                                    LTy::F32 => {
                                        ops.push(StoreF32(Addr(reg.clone(), acc), reg!(var)))
                                    }
                                    LTy::F64 => {
                                        ops.push(StoreF64(Addr(reg.clone(), acc), reg!(var)))
                                    }
                                    LTy::Ptr => {
                                        ops.push(StoreI32(Addr(reg.clone(), acc), reg!(var)))
                                    }
                                    LTy::FPtr => {
                                        ops.push(StoreI32(Addr(reg.clone(), acc), reg!(var)))
                                    }
                                }
                                acc += 8;
                            }
                        }
                        &m::BuiltinCall {
                            ref var,
                            ref fun,
                            ref args,
                            ..
                        } => {
                            let args = args.iter().map(|a| reg!(a)).collect();
                            ops.push(BuiltinCall(reg!(var), fun.clone(), args))
                        }
                        &m::Call {
                            ref var,
                            ref fun,
                            ref args,
                            ..
                        } => {
                            let args = args.iter().map(|a| reg!(a)).collect();
                            match symbol_table.get(fun) {
                                Some(r) => ops.push(ClosureCall(reg!(var), r.clone(), args)),
                                None => ops.push(FunCall(reg!(var), fun.clone(), args)),
                            }
                        }
                        &m::Branch {
                            ref cond,
                            ref clauses,
                            ref default,
                            ..
                        } => {
                            let mut clauses = clauses.clone();
                            clauses.sort_by_key(|&(ref key, _, _)| *key);
                            let default_label = match default.clone() {
                                None => None,
                                Some((label, _)) => {
                                    let params = &target_table[&label];
                                    assert_eq!(params.len(), 1);
                                    let p = &params[0];
                                    match p.0 {
                                        LTy::Unit => {
                                            // do nothing
                                            ()
                                        }
                                        LTy::I32 => ops.push(MoveI32(p.clone(), reg!(cond))),
                                        LTy::I64 => ops.push(MoveI64(p.clone(), reg!(cond))),
                                        LTy::F32 => ops.push(MoveF32(p.clone(), reg!(cond))),
                                        LTy::F64 => ops.push(MoveF64(p.clone(), reg!(cond))),
                                        LTy::Ptr => ops.push(MoveI32(p.clone(), reg!(cond))),
                                        LTy::FPtr => ops.push(MoveI32(p.clone(), reg!(cond))),
                                    };
                                    Some(Label(label))
                                }
                            };

                            if !clauses.is_empty()
                                && clauses[0].0 == 0
                                && clauses
                                    .iter()
                                    .enumerate()
                                    .all(|(n, &(ref key, _, _))| n == (*key as usize))
                            {
                                // use jump table
                                ops.push(JumpTableI32(
                                    reg!(cond),
                                    clauses
                                        .into_iter()
                                        .map(|(_, label, _)| Label(label))
                                        .collect(),
                                    default_label,
                                ))
                            } else {
                                let cond = reg!(cond);
                                // currently supports only i32 types
                                assert_eq!(cond.0, LTy::I32);
                                let boolean = new_reg(LTy::I32);
                                let constant = new_reg(LTy::I32);
                                for (key, label, _) in clauses {
                                    ops.push(ConstI32(constant.clone(), key as u32));
                                    ops.push(EqI32(
                                        boolean.clone(),
                                        cond.clone(),
                                        constant.clone(),
                                    ));
                                    ops.push(JumpIfI32(boolean.clone(), Label(label)))
                                }
                                for label in default_label {
                                    ops.push(Jump(label))
                                }
                            }
                        }
                        &m::Jump {
                            ref target,
                            ref args,
                            ..
                        } => {
                            let params = &target_table[target];
                            for (p, a) in params.iter().zip(args) {
                                match p.0 {
                                    LTy::Unit => {
                                        // do nothing
                                        ()
                                    }
                                    LTy::I32 => ops.push(MoveI32(p.clone(), reg!(a))),
                                    LTy::I64 => ops.push(MoveI64(p.clone(), reg!(a))),
                                    LTy::F32 => ops.push(MoveF32(p.clone(), reg!(a))),
                                    LTy::F64 => ops.push(MoveF64(p.clone(), reg!(a))),
                                    LTy::Ptr => ops.push(MoveI32(p.clone(), reg!(a))),
                                    LTy::FPtr => ops.push(MoveI32(p.clone(), reg!(a))),
                                }
                            }
                            ops.push(Jump(Label(target.clone())))
                        }
                        &m::Ret { ref value, .. } => {
                            ops.push(Ret(value.as_ref().map(|v| reg!(v))));
                        }
                    }
                }
                blocks.push(Block {
                    name: Label(ebb.name.clone()),
                    body: ops,
                })
            }
        }

        let regs = regs.into_iter().map(|r| r.0.clone()).collect::<Vec<_>>();

        Function {
            name: name,
            nparams: nparams,
            regs: regs,
            ret_ty: ret_ty,
            body: blocks,
        }
    }

    fn make_symbol_table<'a, F>(
        &self,
        body: &'a [mir::EBB],
        mut new_reg: F,
    ) -> HashMap<&'a Symbol, Reg>
    where
        F: FnMut(LTy) -> Reg,
    {
        let mut table = HashMap::new();
        macro_rules! intern {
            ($ty: expr, $var: expr) => {{
                if table.get(&$var).is_none() {
                    table.insert($var, new_reg($ty));
                };
            }};
        }

        // allocate function params first
        for &(ref ty, ref param) in &body[0].params {
            intern!(self.ebbty_to_lty(ty), param);
        }

        for ebb in body {
            for &(ref ty, ref param) in &ebb.params {
                intern!(self.ebbty_to_lty(ty), param);
            }

            for op in ebb.body.iter() {
                match op {
                    &mir::Op::Lit {
                        ref var, ref ty, ..
                    }
                    | &mir::Op::Alias {
                        ref var, ref ty, ..
                    }
                    | &mir::Op::Add {
                        ref var, ref ty, ..
                    }
                    | &mir::Op::Sub {
                        ref var, ref ty, ..
                    }
                    | &mir::Op::Mul {
                        ref var, ref ty, ..
                    }
                    | &mir::Op::DivInt {
                        ref var, ref ty, ..
                    }
                    | &mir::Op::DivFloat {
                        ref var, ref ty, ..
                    }
                    | &mir::Op::Mod {
                        ref var, ref ty, ..
                    }
                    | &mir::Op::Eq {
                        ref var, ref ty, ..
                    }
                    | &mir::Op::Neq {
                        ref var, ref ty, ..
                    }
                    | &mir::Op::Gt {
                        ref var, ref ty, ..
                    }
                    | &mir::Op::Ge {
                        ref var, ref ty, ..
                    }
                    | &mir::Op::Lt {
                        ref var, ref ty, ..
                    }
                    | &mir::Op::Le {
                        ref var, ref ty, ..
                    }
                    | &mir::Op::Proj {
                        ref var, ref ty, ..
                    }
                    | &mir::Op::BuiltinCall {
                        ref var, ref ty, ..
                    }
                    | &mir::Op::Call {
                        ref var, ref ty, ..
                    } => {
                        intern!(self.ebbty_to_lty(ty), var);
                    }
                    &mir::Op::Tuple { ref var, .. } | &mir::Op::Closure { ref var, .. } => {
                        intern!(LTy::Ptr, var);
                    }
                    _ => (),
                }
            }
        }

        table
    }

    fn make_target_table<'a>(
        &self,
        body: &'a [mir::EBB],
        symbol_table: &HashMap<&'a Symbol, Reg>,
    ) -> HashMap<&'a Symbol, Vec<Reg>> {
        let mut tbl = HashMap::new();
        for ebb in body {
            let params = ebb
                .params
                .iter()
                .map(|&(_, ref param)| symbol_table[param].clone())
                .collect();
            tbl.insert(&ebb.name, params);
        }
        tbl
    }
}

impl<E> Pass<mir::MIR, E> for MIR2LIR {
    type Target = LIR;

    fn trans(&mut self, mir: mir::MIR, _: &Config) -> ::std::result::Result<Self::Target, E> {
        Ok(self.trans_mir(mir))
    }
}
