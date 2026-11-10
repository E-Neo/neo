use crate::check::{self, Ctx};
use crate::conv::{conv, conv_ty};
use crate::core::{Branch, RcTm, Tm, abstract_term, app, const_, eq, ind, lam, pi, rc, shift, var};
use crate::env::{CtorInfo, EnumInfo, GlobalDef, Globals, Sort};
use crate::eval::{apply_pi, eval, eval_closure, force, vapp};
use crate::quote::quote;
use crate::surface::*;
use crate::value::*;
use neo_common::error::{Error, Result, Span};

/// Elaborate and register every declaration in the program.
pub fn elaborate_program(globals: &mut Globals, program: &Program) -> Result<()> {
    for decl in &program.decls {
        match decl {
            Decl::Enum(e) => elab_enum(globals, e)?,
            Decl::Fn(f) => elab_fn(globals, f)?,
        }
    }
    crate::recursion::check_program(globals)?;
    Ok(())
}

fn show_sort(sort: Sort) -> String {
    match sort {
        Sort::Prop => "Prop".to_string(),
        Sort::Type(u) => format!("Type {}", u),
    }
}

fn elab_enum(globals: &mut Globals, decl: &EnumDecl) -> Result<()> {
    let result = elab_enum_inner(globals, decl);
    if result.is_err() {
        globals.enums.remove(&decl.name);
    }
    result
}

fn elab_enum_inner(globals: &mut Globals, decl: &EnumDecl) -> Result<()> {
    if globals.enums.contains_key(&decl.name) || globals.defs.contains_key(&decl.name) {
        return Err(Error::at(
            decl.span,
            format!("`{}` is already defined", decl.name),
        ));
    }

    let sort = match &decl.sort {
        Some(e) => {
            let mut ctx = Ctx::new();
            let tm = elab_type(globals, &mut ctx, e)?;
            check::infer_sort(globals, &mut Ctx::new(), &tm).map_err(|err| {
                Error::at(e.span(), format!("invalid enum sort: {}", err.message))
            })?;
            let v = force(globals, eval(globals, &ctx.env, &tm));
            match &*v {
                Val::Type(u) => Sort::Type(*u),
                Val::Prop => Sort::Prop,
                _ => {
                    return Err(Error::at(
                        e.span(),
                        "an enum sort must be `Prop` or `Type u`",
                    ));
                }
            }
        }
        None => Sort::Type(0),
    };

    let mut pctx = Ctx::new();
    let mut param_names = Vec::new();
    let mut param_tys = Vec::new();
    for p in &decl.params {
        let tm = elab_type(globals, &mut pctx, &p.ty)?;
        check::infer_sort(globals, &mut pctx, &tm)
            .map_err(|err| Error::at(p.span, format!("invalid enum parameter: {}", err.message)))?;
        let v = eval(globals, &pctx.env, &tm);
        param_names.push(p.name.clone());
        param_tys.push(tm);
        pctx.push(p.name.clone(), v);
    }
    let np = param_names.len();

    // Register the enum before its constructors so that recursive occurrences
    // in field types resolve.
    globals.enums.insert(
        decl.name.clone(),
        EnumInfo {
            name: decl.name.clone(),
            param_names: param_names.clone(),
            param_tys: param_tys.clone(),
            sort,
            ctors: Vec::new(),
            prop_eliminable: false,
        },
    );

    let mut ctor_infos: Vec<CtorInfo> = Vec::new();
    let mut ctor_singleton_ok = Vec::new();
    for (idx, c) in decl.ctors.iter().enumerate() {
        if globals.defs.contains_key(&c.name) || globals.enums.contains_key(&c.name) {
            return Err(Error::at(
                c.span,
                format!("`{}` is already defined", c.name),
            ));
        }
        if ctor_infos.iter().any(|info| info.name == c.name) {
            return Err(Error::at(
                c.span,
                format!("constructor `{}` is defined twice", c.name),
            ));
        }
        let nf = c.fields.len();
        let mut ctx = Ctx::new();
        for (name, ty) in param_names.iter().zip(param_tys.iter()) {
            let v = eval(globals, &ctx.env, ty);
            ctx.push(name.clone(), v);
        }

        let mut field_names = Vec::new();
        let mut field_tms = Vec::new();
        let mut all_fields_prop = true;
        for (j, f) in c.fields.iter().enumerate() {
            let tm = elab_type(globals, &mut ctx, &f.ty)?;
            crate::positivity::check_field(globals, &decl.name, &tm)?;
            let field_sort = check::infer_sort(globals, &mut ctx, &tm).map_err(|e| {
                Error::at(f.span, format!("invalid constructor field: {}", e.message))
            })?;
            if let Sort::Type(u) = sort
                && field_sort.level() > u
            {
                return Err(Error::at(
                    f.span,
                    format!(
                        "constructor field lives in `{}`, but `{}` is declared in `Type {}`; a field may not be larger than its enum",
                        show_sort(field_sort),
                        decl.name,
                        u
                    ),
                ));
            }
            all_fields_prop &= field_sort == Sort::Prop;
            let v = eval(globals, &ctx.env, &tm);
            let fname = f.name.clone().unwrap_or_else(|| format!("_f{}", j));
            field_names.push(fname.clone());
            field_tms.push(tm);
            ctx.push(fname, v);
        }

        let result_tm = match &c.result {
            Some(e) => elab_type(globals, &mut ctx, e)?,
            None => {
                let args = (0..np).map(|j| var(np + nf - 1 - j)).collect();
                ind(decl.name.clone(), args)
            }
        };

        if c.result.is_some() {
            let result_v = force(globals, eval(globals, &ctx.env, &result_tm));
            let valid = match &*result_v {
                Val::Ind(name, args) => name == &decl.name && args.len() == np,
                _ => false,
            };
            if !valid {
                let shown = crate::pretty::show_val(globals, ctx.level(), &result_v);
                return Err(Error::at(
                    c.span,
                    format!(
                        "constructor `{}` must return `{}`, but returns `{}`",
                        c.name, decl.name, shown
                    ),
                ));
            }
            // Conservative positivity and universe checks for the indices.
            if let Val::Ind(_, args) = &*result_v {
                for arg in args {
                    let tm = quote(globals, ctx.level(), arg);
                    crate::positivity::check_index(globals, &decl.name, &tm)
                        .map_err(|e| Error::at(c.span, e.message))?;
                    let index_sort = check::infer_sort(globals, &mut ctx, &tm).map_err(|e| {
                        Error::at(c.span, format!("invalid constructor index: {}", e.message))
                    })?;
                    if let Sort::Type(u) = sort
                        && index_sort.level() > u
                    {
                        return Err(Error::at(
                            c.span,
                            format!(
                                "constructor index lives in `{}`, but `{}` is declared in `Type {}`; an index may not be larger than its enum",
                                show_sort(index_sort),
                                decl.name,
                                u
                            ),
                        ));
                    }
                }
            }
        }

        let mut ty = result_tm;
        for i in (0..nf).rev() {
            ty = pi(field_names[i].clone(), field_tms[i].clone(), ty);
        }
        for i in (0..np).rev() {
            ty = pi(param_names[i].clone(), param_tys[i].clone(), ty);
        }

        // Sanity-check that the constructor type is well formed and closed.
        let mut empty = Ctx::new();
        check::infer_sort(globals, &mut empty, &ty).map_err(|e| {
            Error::at(
                c.span,
                format!("invalid constructor `{}`: {}", c.name, e.message),
            )
        })?;

        ctor_infos.push(CtorInfo {
            name: c.name.clone(),
            enum_name: decl.name.clone(),
            index: idx,
            num_params: np,
            num_fields: nf,
            ty,
        });
        ctor_singleton_ok.push(all_fields_prop);
    }

    // Singleton elimination: a `Prop` enum can be eliminated into a `Type`
    // only when it has no constructors, or exactly one constructor whose
    // fields are all propositions.
    let prop_eliminable = sort == Sort::Prop
        && (ctor_singleton_ok.is_empty() || (ctor_singleton_ok.len() == 1 && ctor_singleton_ok[0]));
    let enum_info = globals.enums.get_mut(&decl.name).unwrap();
    enum_info.ctors = ctor_infos.clone();
    enum_info.prop_eliminable = prop_eliminable;
    for info in ctor_infos {
        globals.defs.insert(
            info.name.clone(),
            GlobalDef::Ctor {
                enum_name: info.enum_name.clone(),
                index: info.index,
                ty: info.ty.clone(),
                arity: info.arity(),
            },
        );
    }
    Ok(())
}

struct FnTypeInfo {
    ty: RcTm,
    domains: Vec<(String, RcTm)>,
    ret: RcTm,
}

fn build_fn_type(globals: &Globals, decl: &FnDecl) -> Result<FnTypeInfo> {
    let mut ctx = Ctx::new();
    let mut domains = Vec::new();
    for p in &decl.params {
        let tm = elab_type(globals, &mut ctx, &p.ty)?;
        let v = eval(globals, &ctx.env, &tm);
        domains.push((p.name.clone(), tm));
        ctx.push(p.name.clone(), v);
    }
    let ret = elab_type(globals, &mut ctx, &decl.ret)?;
    let mut ty = ret.clone();
    for (name, dom) in domains.iter().rev() {
        ty = pi(name.clone(), dom.clone(), ty);
    }
    Ok(FnTypeInfo { ty, domains, ret })
}

fn elab_fn(globals: &mut Globals, decl: &FnDecl) -> Result<()> {
    if globals.defs.contains_key(&decl.name) || globals.enums.contains_key(&decl.name) {
        return Err(Error::at(
            decl.span,
            format!("`{}` is already defined", decl.name),
        ));
    }

    let info = build_fn_type(globals, decl)?;
    globals.add_fn_placeholder(&decl.name, info.ty.clone(), info.domains.len());

    let result = elab_fn_body(globals, decl, &info);
    if result.is_err() {
        // Do not leave a bodyless placeholder behind after a failed definition.
        globals.defs.remove(&decl.name);
    }
    result
}

fn elab_fn_body(globals: &mut Globals, decl: &FnDecl, info: &FnTypeInfo) -> Result<()> {
    let mut ctx = Ctx::new();
    for (name, dom) in &info.domains {
        let dom_v = eval(globals, &ctx.env, dom);
        ctx.push(name.clone(), dom_v);
    }
    let ret_v = eval(globals, &ctx.env, &info.ret);
    let body_expr = Expr::Block(decl.body.clone());
    let mut body = elab_check(globals, &mut ctx, &body_expr, &ret_v)?;
    for (name, dom) in info.domains.iter().rev() {
        body = lam(name.clone(), dom.clone(), body);
    }

    let mut type_ctx = Ctx::new();
    check::infer_sort(globals, &mut type_ctx, &info.ty).map_err(|e| {
        Error::at(
            decl.span,
            format!("kernel rejected the type of `{}`: {}", decl.name, e.message),
        )
    })?;
    let ty_v = eval(globals, &Vec::new(), &info.ty);
    let mut body_ctx = Ctx::new();
    check::check(globals, &mut body_ctx, &body, &ty_v).map_err(|e| {
        Error::at(
            decl.span,
            format!("kernel rejected `{}`: {}", decl.name, e.message),
        )
    })?;

    // The authoritative termination check runs on the elaborated core term,
    // while the definition's own body is still opaque.
    crate::recursion::check_def(globals, &decl.name, &body).map_err(|e| {
        Error::at(
            decl.span,
            format!("kernel rejected `{}`: {}", decl.name, e.message),
        )
    })?;

    globals.set_fn_body(&decl.name, body);
    Ok(())
}

fn elab_type(globals: &Globals, ctx: &mut Ctx, expr: &Expr) -> Result<RcTm> {
    let (tm, ty) = elab_infer(globals, ctx, expr)?;
    check::sort_of_val_in(globals, ctx, &ty)
        .map_err(|e| Error::at(expr.span(), format!("expected a type: {}", e.message)))?;
    Ok(tm)
}

pub fn elab_infer(globals: &Globals, ctx: &mut Ctx, expr: &Expr) -> Result<(RcTm, V)> {
    match expr {
        Expr::Var(name, span) => elab_var(globals, ctx, name, *span),
        Expr::Nat(n, span) => {
            let tm = nat_literal(globals, *n, *span)?;
            Ok((tm, vind("Nat", Vec::new())))
        }
        Expr::Type(u, _) => Ok((rc(Tm::Type(*u)), vtype(u + 1))),
        Expr::Prop(_) => Ok((rc(Tm::Prop), vtype(0))),
        Expr::FnType(binders, body, _) => elab_pi(globals, ctx, binders, body),
        Expr::FnLam(binders, body, span) => elab_lam_infer(globals, ctx, binders, body, *span),
        Expr::App(f, args, span) => {
            if let Expr::Var(n, _) = &**f
                && ctx.lookup_index(n).is_none()
                && globals.is_constructor(n)
            {
                return elab_ctor_infer(globals, ctx, n, args, *span);
            }
            elab_app(globals, ctx, f, args, *span)
        }
        Expr::Match(scrut, arms, span) => {
            let (tm, ty) = elab_match(globals, ctx, scrut, arms, None, *span)?;
            Ok((tm, ty))
        }
        Expr::Eq(lhs, rhs, _) => elab_eq(globals, ctx, lhs, rhs),
        Expr::Refl(span) => Err(Error::at(
            *span,
            "`refl` needs an expected equality type; use it where the type is known",
        )),
        Expr::Block(b) => elab_block_infer(globals, ctx, b),
        Expr::Paren(e, _) => elab_infer(globals, ctx, e),
    }
}

fn elab_var(globals: &Globals, ctx: &Ctx, name: &str, span: Span) -> Result<(RcTm, V)> {
    if let Some(idx) = ctx.lookup_index(name) {
        let ty = ctx
            .local_ty(idx)
            .cloned()
            .ok_or_else(|| Error::at(span, format!("unbound variable `{}`", name)))?;
        return Ok((var(idx), ty));
    }
    if let Some(info) = globals.get_enum(name) {
        if info.param_tys.is_empty() {
            let ty = check::sort_value(info.sort);
            return Ok((ind(name, Vec::new()), ty));
        }
        return Err(Error::at(
            span,
            format!(
                "type constructor `{}` expects {} argument(s)",
                name,
                info.param_tys.len()
            ),
        ));
    }
    if let Some(ty) = globals.get_ty(name) {
        let ty = eval(globals, &Vec::new(), ty);
        return Ok((const_(name), ty));
    }
    Err(Error::at(span, format!("unknown name `{}`", name)))
}

fn elab_enum_app(
    globals: &Globals,
    ctx: &mut Ctx,
    name: &str,
    args: &[Expr],
    span: Span,
) -> Result<(RcTm, V)> {
    let info = globals.get_enum(name).unwrap();
    let param_tys = info.param_tys.clone();
    let sort = info.sort;
    if args.len() != param_tys.len() {
        return Err(Error::at(
            span,
            format!(
                "`{}` expects {} argument(s), got {}",
                name,
                param_tys.len(),
                args.len()
            ),
        ));
    }
    let mut local_env: Env = Vec::new();
    let mut terms = Vec::new();
    for (arg, pty) in args.iter().zip(param_tys.iter()) {
        let dom = eval(globals, &local_env, pty);
        let tm = elab_check(globals, ctx, arg, &dom)?;
        let v = eval(globals, &ctx.env, &tm);
        local_env.push(v);
        terms.push(tm);
    }
    Ok((ind(name, terms), check::sort_value(sort)))
}

fn elab_app(
    globals: &Globals,
    ctx: &mut Ctx,
    f: &Expr,
    args: &[Expr],
    span: Span,
) -> Result<(RcTm, V)> {
    if let Expr::Var(name, _) = f
        && globals.get_enum(name).is_some()
    {
        return elab_enum_app(globals, ctx, name, args, span);
    }
    let (mut f_tm, mut f_ty) = elab_infer(globals, ctx, f)?;
    for arg in args {
        let ft = force(globals, f_ty);
        let (dom, closure) = match &*ft {
            Val::Pi(dom, closure) => (dom.clone(), closure.clone()),
            _ => {
                return Err(Error::at(
                    arg.span(),
                    "application: the function has a non-function type",
                ));
            }
        };
        let arg_tm = elab_check(globals, ctx, arg, &dom)?;
        let arg_v = eval(globals, &ctx.env, &arg_tm);
        f_tm = app(f_tm, arg_tm);
        f_ty = eval_closure(globals, &closure, arg_v);
    }
    Ok((f_tm, f_ty))
}

fn elab_pi(globals: &Globals, ctx: &mut Ctx, binders: &[Binder], body: &Expr) -> Result<(RcTm, V)> {
    let mut doms = Vec::new();
    for b in binders {
        let tm = elab_type(globals, ctx, &b.ty)?;
        let v = eval(globals, &ctx.env, &tm);
        let name = b
            .name
            .clone()
            .unwrap_or_else(|| format!("_x{}", doms.len()));
        ctx.push(name.clone(), v);
        doms.push((name, tm));
    }
    let cod = elab_type(globals, ctx, body)?;
    for _ in 0..doms.len() {
        ctx.pop();
    }
    let mut tm = cod;
    for (name, dom) in doms.into_iter().rev() {
        tm = pi(name, dom, tm);
    }
    let ty = check::infer(globals, ctx, &tm)?;
    Ok((tm, ty))
}

fn elab_lam_infer(
    globals: &Globals,
    ctx: &mut Ctx,
    binders: &[Binder],
    body: &Expr,
    span: Span,
) -> Result<(RcTm, V)> {
    let mut doms = Vec::new();
    for b in binders {
        let tm = elab_type(globals, ctx, &b.ty)?;
        let v = eval(globals, &ctx.env, &tm);
        let name = b
            .name
            .clone()
            .unwrap_or_else(|| format!("_x{}", doms.len()));
        ctx.push(name.clone(), v);
        doms.push((name, tm));
    }
    if doms.is_empty() {
        return Err(Error::at(span, "a lambda must bind at least one argument"));
    }
    let (body_tm, body_ty) = elab_infer(globals, ctx, body)?;
    let cod = quote(globals, ctx.level(), &body_ty);
    for _ in 0..doms.len() {
        ctx.pop();
    }
    let mut tm = body_tm;
    let mut ty_tm = cod;
    for (name, dom) in doms.into_iter().rev() {
        tm = lam(name.clone(), dom.clone(), tm);
        ty_tm = pi(name, dom, ty_tm);
    }
    let ty = eval(globals, &ctx.env, &ty_tm);
    Ok((tm, ty))
}

fn elab_eq(globals: &Globals, ctx: &mut Ctx, lhs: &Expr, rhs: &Expr) -> Result<(RcTm, V)> {
    let (lhs_tm, lhs_ty) = elab_infer(globals, ctx, lhs)?;
    let rhs_tm = elab_check(globals, ctx, rhs, &lhs_ty)?;
    let ty_tm = quote(globals, ctx.level(), &lhs_ty);
    Ok((eq(ty_tm, lhs_tm, rhs_tm), vprop()))
}

fn elab_block_infer(globals: &Globals, ctx: &mut Ctx, block: &Block) -> Result<(RcTm, V)> {
    let saved = elab_bindings(globals, ctx, block)?;
    let (tail_tm, _tail_ty) = elab_infer(globals, ctx, &block.tail)?;
    for _ in &block.bindings {
        ctx.pop();
    }
    let mut tm = tail_tm;
    for (name, dom, value) in saved.into_iter().rev() {
        tm = app(lam(name, dom, tm), value);
    }
    let ty = check::infer(globals, ctx, &tm)?;
    Ok((tm, ty))
}

fn elab_block_check(globals: &Globals, ctx: &mut Ctx, block: &Block, expected: &V) -> Result<RcTm> {
    let saved = elab_bindings(globals, ctx, block)?;
    let tail_tm = elab_check(globals, ctx, &block.tail, expected)?;
    for _ in &block.bindings {
        ctx.pop();
    }
    let mut tm = tail_tm;
    for (name, dom, value) in saved.into_iter().rev() {
        tm = app(lam(name, dom, tm), value);
    }
    Ok(tm)
}

fn elab_bindings(
    globals: &Globals,
    ctx: &mut Ctx,
    block: &Block,
) -> Result<Vec<(String, RcTm, RcTm)>> {
    let mut saved = Vec::new();
    for b in &block.bindings {
        let (value_tm, value_ty) = elab_infer(globals, ctx, &b.value)?;
        let dom = quote(globals, ctx.level(), &value_ty);
        saved.push((b.name.clone(), dom, value_tm));
        ctx.push(b.name.clone(), value_ty);
    }
    Ok(saved)
}

pub fn elab_check(globals: &Globals, ctx: &mut Ctx, expr: &Expr, expected: &V) -> Result<RcTm> {
    match expr {
        Expr::FnLam(binders, body, span) => {
            elab_lam_check(globals, ctx, binders, body, expected, *span)
        }
        Expr::Refl(span) => elab_refl(globals, ctx, expected, *span),
        Expr::Var(n, span) => {
            if ctx.lookup_index(n).is_none() && globals.is_constructor(n) {
                elab_ctor_check(globals, ctx, n, &[], expected, *span)
            } else {
                infer_and_check(globals, ctx, expr, expected)
            }
        }
        Expr::App(f, args, span) => {
            if let Expr::Var(n, _) = &**f
                && ctx.lookup_index(n).is_none()
                && globals.is_constructor(n)
            {
                return elab_ctor_check(globals, ctx, n, args, expected, *span);
            }
            infer_and_check(globals, ctx, expr, expected)
        }
        Expr::Match(scrut, arms, span) => {
            let (tm, _ty) = elab_match(globals, ctx, scrut, arms, Some(expected), *span)?;
            Ok(tm)
        }
        Expr::Block(b) => elab_block_check(globals, ctx, b, expected),
        Expr::Paren(e, _) => elab_check(globals, ctx, e, expected),
        _ => infer_and_check(globals, ctx, expr, expected),
    }
}

fn infer_and_check(globals: &Globals, ctx: &mut Ctx, expr: &Expr, expected: &V) -> Result<RcTm> {
    let (tm, ty) = elab_infer(globals, ctx, expr)?;
    if conv_ty(globals, ctx.level(), &ty, expected) {
        Ok(tm)
    } else {
        let found = crate::pretty::show_val(globals, ctx.level(), &ty);
        let want = crate::pretty::show_val(globals, ctx.level(), expected);
        Err(Error::at(
            expr.span(),
            format!("type mismatch: expected `{}`, found `{}`", want, found),
        ))
    }
}

fn elab_ctor_fields(
    globals: &Globals,
    ctx: &mut Ctx,
    ci: &CtorInfo,
    param_vals: &[V],
    args: &[Expr],
    span: Span,
) -> Result<(Vec<RcTm>, V)> {
    if args.len() != ci.num_fields {
        return Err(Error::at(
            span,
            format!(
                "constructor `{}` takes {} field(s), got {}",
                ci.name,
                ci.num_fields,
                args.len()
            ),
        ));
    }
    let mut field_ty = eval(globals, &Vec::new(), &ci.ty);
    for p in param_vals {
        field_ty = apply_pi(globals, field_ty, p.clone())
            .ok_or_else(|| Error::new("constructor parameter mismatch"))?;
    }
    let mut field_tms = Vec::new();
    for arg in args {
        let ft = force(globals, field_ty);
        let (dom, closure) = match &*ft {
            Val::Pi(dom, closure) => (dom.clone(), closure.clone()),
            _ => return Err(Error::new("constructor field mismatch")),
        };
        let tm = elab_check(globals, ctx, arg, &dom)?;
        let v = eval(globals, &ctx.env, &tm);
        field_ty = eval_closure(globals, &closure, v);
        field_tms.push(tm);
    }
    Ok((field_tms, field_ty))
}

fn elab_ctor_check(
    globals: &Globals,
    ctx: &mut Ctx,
    name: &str,
    args: &[Expr],
    expected: &V,
    span: Span,
) -> Result<RcTm> {
    let ci = globals
        .get_ctor(name)
        .cloned()
        .ok_or_else(|| Error::at(span, format!("unknown constructor `{}`", name)))?;
    let expected_f = force(globals, expected.clone());
    let param_vals = match &*expected_f {
        Val::Ind(en, ps) if *en == ci.enum_name => ps.clone(),
        _ => {
            let want = crate::pretty::show_val(globals, ctx.level(), expected);
            return Err(Error::at(
                span,
                format!(
                    "cannot infer the type arguments of `{}` here; expected `{}`",
                    name, want
                ),
            ));
        }
    };
    let (field_tms, result) = elab_ctor_fields(globals, ctx, &ci, &param_vals, args, span)?;
    if !conv(globals, ctx.level(), &result, &expected_f) {
        return Err(Error::at(
            span,
            format!("constructor `{}` does not produce the expected type", name),
        ));
    }
    let mut all: Vec<RcTm> = param_vals
        .iter()
        .map(|p| quote(globals, ctx.level(), p))
        .collect();
    all.extend(field_tms);
    Ok(rc(Tm::Ctor(ci.enum_name.clone(), ci.index, all)))
}

fn elab_ctor_infer(
    globals: &Globals,
    ctx: &mut Ctx,
    name: &str,
    args: &[Expr],
    span: Span,
) -> Result<(RcTm, V)> {
    let ci = globals
        .get_ctor(name)
        .cloned()
        .ok_or_else(|| Error::at(span, format!("unknown constructor `{}`", name)))?;
    if ci.num_params != 0 {
        return Err(Error::at(
            span,
            format!(
                "cannot infer the type arguments of `{}`; use it where the expected type is known",
                name
            ),
        ));
    }
    let (field_tms, result) = elab_ctor_fields(globals, ctx, &ci, &[], args, span)?;
    Ok((
        rc(Tm::Ctor(ci.enum_name.clone(), ci.index, field_tms)),
        result,
    ))
}

fn elab_refl(globals: &Globals, ctx: &mut Ctx, expected: &V, span: Span) -> Result<RcTm> {
    let expected = force(globals, expected.clone());
    let (lhs, rhs) = match &*expected {
        Val::Eq(_, lhs, rhs) => (lhs.clone(), rhs.clone()),
        _ => return Err(Error::at(span, "`refl` can only prove an equality")),
    };
    if !conv(globals, ctx.level(), &lhs, &rhs) {
        let l = crate::pretty::show_val(globals, ctx.level(), &lhs);
        let r = crate::pretty::show_val(globals, ctx.level(), &rhs);
        return Err(Error::at(
            span,
            format!("`refl` cannot prove `{} == {}` (sides are not equal)", l, r),
        ));
    }
    Ok(rc(Tm::Refl(quote(globals, ctx.level(), &lhs))))
}

fn elab_lam_check(
    globals: &Globals,
    ctx: &mut Ctx,
    binders: &[Binder],
    body: &Expr,
    expected: &V,
    span: Span,
) -> Result<RcTm> {
    let mut pushed = 0;
    let mut doms = Vec::new();
    let mut expected = expected.clone();
    for b in binders {
        let exp = force(globals, expected);
        let (dom_v, closure) = match &*exp {
            Val::Pi(dom, closure) => (dom.clone(), closure.clone()),
            _ => {
                return Err(Error::at(
                    span,
                    "this lambda has more binders than its expected function type",
                ));
            }
        };
        let dom_tm = elab_type(globals, ctx, &b.ty)?;
        let annotated = eval(globals, &ctx.env, &dom_tm);
        if !conv(globals, ctx.level(), &annotated, &dom_v) {
            return Err(Error::at(
                b.span,
                "lambda binder type does not match the expected function type",
            ));
        }
        let name = b
            .name
            .clone()
            .unwrap_or_else(|| format!("_x{}", doms.len()));
        let fresh = ctx.push(name.clone(), dom_v);
        doms.push((name, dom_tm));
        expected = eval_closure(globals, &closure, fresh);
        pushed += 1;
    }
    let body_tm = elab_check(globals, ctx, body, &expected)?;
    for _ in 0..pushed {
        ctx.pop();
    }
    let mut tm = body_tm;
    for (name, dom) in doms.into_iter().rev() {
        tm = lam(name, dom, tm);
    }
    Ok(tm)
}

fn nat_literal(globals: &Globals, n: u64, span: Span) -> Result<RcTm> {
    let info = globals
        .get_enum("Nat")
        .ok_or_else(|| Error::at(span, "numeric literals require the `Nat` type"))?;
    let zero = info
        .ctors
        .iter()
        .position(|c| c.name == "zero")
        .ok_or_else(|| Error::at(span, "`Nat` must have a `zero` constructor"))?;
    let succ = info
        .ctors
        .iter()
        .position(|c| c.name == "succ")
        .ok_or_else(|| Error::at(span, "`Nat` must have a `succ` constructor"))?;
    let mut tm = rc(Tm::Ctor("Nat".to_string(), zero, Vec::new()));
    for _ in 0..n {
        tm = rc(Tm::Ctor("Nat".to_string(), succ, vec![tm]));
    }
    Ok(tm)
}

fn elab_match(
    globals: &Globals,
    ctx: &mut Ctx,
    scrut: &Expr,
    arms: &[Arm],
    expected: Option<&V>,
    span: Span,
) -> Result<(RcTm, V)> {
    let (scrut_tm, scrut_ty) = elab_infer(globals, ctx, scrut)?;
    let scrut_ty = force(globals, scrut_ty);

    if let Val::Eq(eq_ty, eq_lhs, eq_rhs) = &*scrut_ty {
        return elab_eq_match(
            globals,
            ctx,
            EqMatchData {
                proof: scrut_tm,
                ty: eq_ty.clone(),
                lhs: eq_lhs.clone(),
                rhs: eq_rhs.clone(),
            },
            arms,
            expected,
            span,
        );
    }

    let (enum_name, params) = match &*scrut_ty {
        Val::Ind(name, args) => (name.clone(), args.clone()),
        _ => {
            return Err(Error::at(
                span,
                "match: the scrutinee is not an enum value or an equality proof",
            ));
        }
    };
    let info = match globals.get_enum(&enum_name) {
        Some(i) => i.clone(),
        None => return Err(Error::at(span, format!("unknown enum `{}`", enum_name))),
    };

    // Refine each constructor's result indices against the scrutinee type.
    // This decides which constructors can produce a value of this type and
    // which field indices are forced by the scrutinee's indices.
    let mut refinements = Vec::with_capacity(info.ctors.len());
    for ctor in &info.ctors {
        refinements.push(check::refine_ctor(globals, ctx, ctor, &params, &scrut_ty)?);
    }

    // A bare identifier naming a nullary constructor is a constructor pattern,
    // not a binder.
    let arms: Vec<Arm> = arms
        .iter()
        .map(|arm| {
            let mut arm = arm.clone();
            if let Pattern::Var(n, s) = &arm.pattern
                && info.ctors.iter().any(|c| c.name == *n && c.num_fields == 0)
            {
                arm.pattern = Pattern::Ctor(n.clone(), Vec::new(), *s);
            }
            arm
        })
        .collect();
    let arms = &arms[..];

    // A single variable/wildcard arm is an irrefutable binding.
    if arms.len() == 1 && matches!(arms[0].pattern, Pattern::Var(..) | Pattern::Wild(..)) {
        let name = match &arms[0].pattern {
            Pattern::Var(n, _) => n.clone(),
            _ => "_".to_string(),
        };
        let dom = quote(globals, ctx.level(), &scrut_ty);
        ctx.push(name.clone(), scrut_ty.clone());
        let body_tm = match expected {
            Some(exp) => elab_check(globals, ctx, &arms[0].body, exp)?,
            None => elab_infer(globals, ctx, &arms[0].body)?.0,
        };
        ctx.pop();
        let tm = app(lam(name, dom, body_tm), scrut_tm);
        let ty = match expected {
            Some(e) => e.clone(),
            None => check::infer(globals, ctx, &tm)?,
        };
        return Ok((tm, ty));
    }

    let mut by_index: Vec<Option<&Arm>> = vec![None; info.ctors.len()];
    for arm in arms {
        match &arm.pattern {
            Pattern::Ctor(cname, subpats, aspan) => {
                let idx = match info.ctors.iter().position(|c| c.name == *cname) {
                    Some(i) => i,
                    None => {
                        return Err(Error::at(
                            *aspan,
                            format!("`{}` is not a constructor of `{}`", cname, enum_name),
                        ));
                    }
                };
                if by_index[idx].is_some() {
                    return Err(Error::at(*aspan, format!("duplicate arm for `{}`", cname)));
                }
                for p in subpats {
                    if matches!(p, Pattern::Ctor(..)) {
                        return Err(Error::at(
                            p.span(),
                            "nested constructor patterns are not supported; bind a variable instead",
                        ));
                    }
                }
                if subpats.len() != info.ctors[idx].num_fields {
                    return Err(Error::at(
                        *aspan,
                        format!(
                            "constructor `{}` has {} field(s), but the pattern binds {}",
                            cname,
                            info.ctors[idx].num_fields,
                            subpats.len()
                        ),
                    ));
                }
                by_index[idx] = Some(arm);
            }
            _ => {
                return Err(Error::at(
                    arm.span,
                    "match arms must use constructor patterns (or a single variable pattern)",
                ));
            }
        }
    }
    // Exhaustiveness: every reachable constructor needs an arm. Arms for
    // impossible constructors are accepted but their bodies are not checked,
    // because no value of this scrutinee type can use them.
    for (i, arm) in by_index.iter().enumerate() {
        if arm.is_none() && refinements[i].possible {
            return Err(Error::at(
                span,
                format!(
                    "match is not exhaustive: missing constructor `{}`",
                    info.ctors[i].name
                ),
            ));
        }
    }

    // Build the motive.
    let (motive, _result_ty) = match expected {
        Some(exp) => {
            let m = build_motive(globals, ctx, &scrut_tm, &scrut_ty, exp);
            (m, exp.clone())
        }
        None => {
            let mut inferred: Option<V> = None;
            for (idx, arm_opt) in by_index.iter().enumerate() {
                let Some(arm) = arm_opt else {
                    continue;
                };
                if !refinements[idx].possible {
                    continue;
                }
                let mut branch = refine_branch(globals, ctx, &refinements[idx], &arm.pattern)?;
                let (_, body_ty) = elab_infer(globals, &mut branch.ctx, &arm.body)?;
                match &inferred {
                    None => inferred = Some(body_ty),
                    Some(t) => {
                        if !conv_ty(globals, ctx.level(), t, &body_ty) {
                            return Err(Error::at(
                                arm.span,
                                "match arms have different result types; add an expected type",
                            ));
                        }
                    }
                }
            }
            let rt = inferred.ok_or_else(|| {
                Error::at(
                    span,
                    "cannot infer the result type of a match with no reachable branches; add an expected type",
                )
            })?;
            let dom = quote(globals, ctx.level(), &scrut_ty);
            let body = quote(globals, ctx.level() + 1, &rt);
            (lam("_", dom, body), rt)
        }
    };

    let motive_v = eval(globals, &ctx.env, &motive);
    let mut branches = Vec::with_capacity(info.ctors.len());
    for (idx, ctor) in info.ctors.iter().enumerate() {
        let refinement = &refinements[idx];
        if !refinement.possible {
            // The constructor cannot produce a value of this scrutinee type.
            // Keep a placeholder so branch order still matches constructor
            // order; the kernel skips it and code generation omits it.
            branches.push(Branch {
                ctor: ctor.name.clone(),
                binders: ctor.num_fields,
                body: rc(Tm::Prop),
                impossible: true,
            });
            continue;
        }
        let arm = by_index[idx]
            .ok_or_else(|| Error::new("internal: reachable constructor has no arm"))?;
        let mut branch = refine_branch(globals, ctx, refinement, &arm.pattern)?;
        let level = branch.ctx.level();
        let mut field_vals = Vec::new();
        for j in 0..ctor.num_fields {
            match &refinement.field_solutions[j] {
                Some(solution) => field_vals.push(solution.clone()),
                None => field_vals.push(vvar(ctx.level() + j)),
            }
        }
        let mut ctor_args = params.clone();
        ctor_args.extend(field_vals);
        let ctor_v = vctor(enum_name.clone(), idx, ctor_args);
        let branch_ty = vapp(globals, motive_v.clone(), ctor_v);
        let branch_ty =
            check::apply_solutions(globals, level, &branch.vars, &branch.solutions, &branch_ty);
        let body_tm = elab_check(globals, &mut branch.ctx, &arm.body, &branch_ty)?;
        let body_tm = substitute_field_solutions(
            globals,
            level,
            ctor.num_fields,
            &refinement.field_solutions,
            body_tm,
        );
        branches.push(Branch {
            ctor: ctor.name.clone(),
            binders: ctor.num_fields,
            body: body_tm,
            impossible: false,
        });
    }

    let scrut_v = eval(globals, &ctx.env, &scrut_tm);
    let tm = rc(Tm::Match {
        motive,
        scrut: scrut_tm,
        branches,
    });
    let ty = vapp(globals, motive_v, scrut_v);
    Ok((tm, ty))
}

/// A branch-local context together with the index solutions that the
/// constructor refinement determined. The solutions live in the extended
/// context (`ctx` after the constructor fields have been pushed).
struct BranchCtx {
    ctx: Ctx,
    vars: Vec<usize>,
    solutions: std::collections::HashMap<usize, V>,
}

/// Extend the outer context with the constructor's fields and apply the index
/// solutions to every local type and to the branch environment.
///
/// The branch body keeps the same de Bruijn layout; only the types of the
/// locals and their evaluation environment are refined, so a variable may be
/// known to equal a constructor field without the term being rewritten.
fn refine_branch(
    globals: &Globals,
    ctx: &Ctx,
    refinement: &check::CtorRefinement,
    pattern: &Pattern,
) -> Result<BranchCtx> {
    let mut branch_ctx = ctx.clone();
    push_ctor_binders(&mut branch_ctx, pattern, &refinement.field_tys)?;
    let inner = ctx.level();
    let level = branch_ctx.level();

    let mut vars = Vec::new();
    let mut solutions = std::collections::HashMap::new();
    for (var, value) in &refinement.context_solutions {
        vars.push(*var);
        solutions.insert(*var, value.clone());
    }
    for (j, solution) in refinement.field_solutions.iter().enumerate() {
        if let Some(value) = solution {
            let var = inner + j;
            vars.push(var);
            solutions.insert(var, value.clone());
        }
    }

    for (_, ty) in branch_ctx.locals.iter_mut() {
        *ty = check::apply_solutions(globals, level, &vars, &solutions, ty);
    }
    for (var, value) in &solutions {
        branch_ctx.env[*var] = value.clone();
    }

    Ok(BranchCtx {
        ctx: branch_ctx,
        vars,
        solutions,
    })
}

/// Replace the branch-local variables of fields that the index refinement
/// solved, so the checked body mentions the same values used to build the
/// constructor in the motive. Runs a few passes to resolve solution chains.
fn substitute_field_solutions(
    globals: &Globals,
    level: usize,
    num_fields: usize,
    solutions: &[Option<V>],
    body: RcTm,
) -> RcTm {
    let mut body = body;
    if solutions.iter().all(|s| s.is_none()) {
        return body;
    }
    for _ in 0..=num_fields {
        for (j, solution) in solutions.iter().enumerate() {
            if let Some(solution) = solution {
                let index = num_fields - 1 - j;
                let replacement = quote(globals, level, solution);
                body = crate::core::subst(&body, index, &replacement);
            }
        }
    }
    body
}

fn push_ctor_binders(ctx: &mut Ctx, pattern: &Pattern, field_tys: &[V]) -> Result<usize> {
    let subpats = match pattern {
        Pattern::Ctor(_, ps, _) => ps.as_slice(),
        _ => return Err(Error::new("internal: expected a constructor pattern")),
    };
    if subpats.len() != field_tys.len() {
        return Err(Error::new("internal: constructor field mismatch"));
    }
    let mut pushed = 0;
    for (j, sp) in subpats.iter().enumerate() {
        let name = match sp {
            Pattern::Var(n, _) => n.clone(),
            Pattern::Wild(_) => format!("_p{}", j),
            Pattern::Ctor(_, _, s) => {
                return Err(Error::at(
                    *s,
                    "nested constructor patterns are not supported",
                ));
            }
        };
        ctx.push(name, field_tys[j].clone());
        pushed += 1;
    }
    Ok(pushed)
}

fn build_motive(globals: &Globals, ctx: &Ctx, scrut_tm: &RcTm, scrut_ty: &V, expected: &V) -> RcTm {
    let level = ctx.level();
    let dom = quote(globals, level, scrut_ty);
    let target = shift(scrut_tm, 1, 0);
    let body = abstract_term(&target, &var(0), &quote(globals, level + 1, expected));
    lam("_", dom, body)
}

struct EqMatchData {
    proof: RcTm,
    ty: V,
    lhs: V,
    rhs: V,
}

fn elab_eq_match(
    globals: &Globals,
    ctx: &mut Ctx,
    eq: EqMatchData,
    arms: &[Arm],
    expected: Option<&V>,
    span: Span,
) -> Result<(RcTm, V)> {
    let EqMatchData {
        proof,
        ty: eq_ty,
        lhs: eq_lhs,
        rhs: eq_rhs,
    } = eq;
    if arms.len() != 1 {
        return Err(Error::at(
            span,
            "a match on an equality proof takes one arm",
        ));
    }
    match &arms[0].pattern {
        Pattern::Var(name, _) if name == "refl" => {}
        Pattern::Wild(_) => {}
        _ => {
            return Err(Error::at(
                arms[0].span,
                "a match on an equality proof must use the pattern `refl`",
            ));
        }
    }
    let ty_tm = quote(globals, ctx.level(), &eq_ty);
    let motive = match expected {
        Some(exp) => abstract_rhs(globals, ctx, &ty_tm, &eq_rhs, exp),
        None => {
            let (_, body_ty) = elab_infer(globals, ctx, &arms[0].body)?;
            let body_tm = quote(globals, ctx.level(), &body_ty);
            lam("_", ty_tm.clone(), body_tm)
        }
    };
    let motive_v = eval(globals, &ctx.env, &motive);
    let refl_expected = vapp(globals, motive_v.clone(), eq_lhs.clone());
    let refl_case = elab_check(globals, ctx, &arms[0].body, &refl_expected)?;
    let result = vapp(globals, motive_v, eq_rhs.clone());
    let tm = rc(Tm::J {
        ty: ty_tm,
        lhs: quote(globals, ctx.level(), &eq_lhs),
        rhs: quote(globals, ctx.level(), &eq_rhs),
        proof,
        motive,
        refl_case,
    });
    Ok((tm, result))
}

/// Build the motive for equality elimination: `fun (_: ty) => result`, with
/// occurrences of the right-hand side abstracted to the fresh variable.
fn abstract_rhs(globals: &Globals, ctx: &Ctx, ty_tm: &RcTm, rhs: &V, result: &V) -> RcTm {
    let level = ctx.level();
    let target = quote(globals, level + 1, rhs);
    let body = abstract_term(&target, &var(0), &quote(globals, level + 1, result));
    lam("_", ty_tm.clone(), body)
}
