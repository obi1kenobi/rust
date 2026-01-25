use rustc_data_structures::fx::FxHashSet;
use rustc_data_structures::thin_vec::ThinVec;
use rustc_hir as hir;
use rustc_hir::def::DefKind;
use rustc_hir::def_id::DefId;
use rustc_hir::{LangItem, OpaqueTyOrigin};
use rustc_infer::infer::region_constraints::GenericKind;
use rustc_middle::ty::{self, AliasTy, ParamTy, Ty, TyCtxt, TypingMode};
use rustc_trait_selection::infer::TyCtxtInferExt;
use rustc_trait_selection::infer::outlives::env::OutlivesEnvironment;
use rustc_trait_selection::regions::OutlivesEnvironmentBuildExt;
use rustdoc_json_types::{GenericBound, Id, Path, TraitBoundModifier};

use crate::clean;
use crate::config::OutputFormat;
use crate::core::DocContext;
use crate::formats::cache::Cache;
use crate::json::JsonRenderer;
use crate::json::conversions::IntoJson;

pub(crate) fn implied_bounds_for_ty<'tcx>(
    target_ty: Ty<'tcx>,
    clauses: &[ty::Clause<'tcx>],
    implicitly_sized: bool,
    explicit_bounds: &[GenericBound],
    owner_def_id: DefId,
    renderer: &JsonRenderer<'tcx>,
) -> Vec<GenericBound> {
    let mut seen: FxHashSet<_> = explicit_bounds.iter().cloned().collect();

    let explicit_trait_bounds: FxHashSet<_> = explicit_bounds
        .iter()
        .filter_map(|bound| match bound {
            GenericBound::TraitBound { trait_, modifier, .. } => Some((trait_.id, *modifier)),
            _ => None,
        })
        .collect();

    let mut implied_bounds = Vec::new();
    let mut added_sized_bound = explicit_bounds.iter().any(|bound| is_sized_bound(bound, renderer));
    let mut clean_cx = implied_bounds_doc_context(renderer, owner_def_id);
    for clause in clauses {
        if !clause_targets_ty(*clause, target_ty) {
            continue;
        }

        if let Some(bound) = clause_to_generic_bound(
            *clause,
            clauses,
            &explicit_trait_bounds,
            &mut clean_cx,
            renderer,
        ) {
            if is_sized_bound(&bound, renderer) {
                added_sized_bound = true;
            }

            if seen.insert(bound.clone()) {
                implied_bounds.push(bound);
            }
        }
    }

    if implicitly_sized && !added_sized_bound {
        if let Some(sized_def_id) = renderer.tcx.lang_items().sized_trait() {
            let sized_id = renderer.id_from_item_default(sized_def_id.into());
            let sized_bound = GenericBound::TraitBound {
                trait_: Path {
                    path: renderer.tcx.item_name(sized_def_id).to_string(),
                    id: sized_id,
                    args: None,
                },
                generic_params: Vec::new(),
                modifier: TraitBoundModifier::None,
            };
            if seen.insert(sized_bound.clone()) {
                implied_bounds.push(sized_bound);
            }
        }
    }

    implied_bounds
}

pub(crate) fn implied_bounds_for_type_param<'tcx>(
    owner_def_id: DefId,
    param_def_id: DefId,
    explicit_bounds: &[GenericBound],
    renderer: &JsonRenderer<'tcx>,
) -> Vec<GenericBound> {
    let Some(target_param) = param_ty_for_param(renderer.tcx, owner_def_id, param_def_id) else {
        return Vec::new();
    };
    let target_ty = target_param.to_ty(renderer.tcx);

    let clauses = renderer.tcx.param_env(owner_def_id).caller_bounds();
    let allows_unsized = explicit_bounds.iter().any(|bound| is_maybe_sized_bound(bound, renderer));
    let implicitly_sized =
        !allows_unsized || param_requires_sized_in_fn_sig(renderer.tcx, owner_def_id, target_param);

    let mut implied_bounds = implied_bounds_for_ty(
        target_ty,
        clauses.as_slice(),
        implicitly_sized,
        explicit_bounds,
        owner_def_id,
        renderer,
    );

    let extra_bounds = implied_outlives_bounds_for_param(renderer.tcx, owner_def_id, target_param);
    if !extra_bounds.is_empty() {
        let mut seen: FxHashSet<_> = explicit_bounds.iter().cloned().collect();
        seen.extend(implied_bounds.iter().cloned());
        for bound in extra_bounds {
            if seen.insert(bound.clone()) {
                implied_bounds.push(bound);
            }
        }
    }

    implied_bounds
}

pub(crate) fn implied_bounds_for_assoc_type<'tcx>(
    assoc_def_id: DefId,
    explicit_bounds: &[GenericBound],
    renderer: &JsonRenderer<'tcx>,
) -> Vec<GenericBound> {
    let assoc_item = renderer.tcx.associated_item(assoc_def_id);
    if !matches!(assoc_item.container, ty::AssocContainer::Trait) {
        return Vec::new();
    }

    let args = ty::GenericArgs::identity_for_item(renderer.tcx, assoc_def_id);
    let target_ty = Ty::new_alias(
        renderer.tcx,
        ty::Projection,
        AliasTy::new_from_args(renderer.tcx, assoc_def_id, args),
    );
    let clauses = renderer.tcx.item_bounds(assoc_def_id).instantiate(renderer.tcx, args);
    let allows_unsized = explicit_bounds.iter().any(|bound| is_maybe_sized_bound(bound, renderer));
    implied_bounds_for_ty(
        target_ty,
        &clauses,
        !allows_unsized,
        explicit_bounds,
        assoc_def_id,
        renderer,
    )
}

pub(crate) fn implied_bounds_for_impl_trait<'tcx>(
    origin: &clean::ImplTraitOrigin,
    explicit_bounds: &[GenericBound],
    renderer: &JsonRenderer<'tcx>,
) -> Vec<GenericBound> {
    match origin {
        clean::ImplTraitOrigin::Param { def_id } => {
            let Some(owner_def_id) = renderer.tcx.opt_parent(*def_id) else { return Vec::new() };
            implied_bounds_for_type_param(owner_def_id, *def_id, explicit_bounds, renderer)
        }
        clean::ImplTraitOrigin::Opaque { def_id, needs_sized_check } => {
            let args = ty::GenericArgs::identity_for_item(renderer.tcx, *def_id);
            let target_ty = Ty::new_alias(
                renderer.tcx,
                ty::Opaque,
                AliasTy::new_from_args(renderer.tcx, *def_id, args),
            );
            let clauses = renderer.tcx.item_bounds(*def_id).instantiate(renderer.tcx, args);

            // If `?Sized` is present, we need to determine whether the use site implies
            // the opaque type is `Sized` or not. The use site check involves a HIR walk,
            // and `?Sized` bounds are relatively rare in Rust, so we prefer to perform
            // the checks in the order that's likely the least work:
            // - If no explicit `?Sized` bound is present, `Sized` is implied by default.
            // - Otherwise, walk the HIR to determine whether `Sized` is implied or not.
            let explicit_maybe_sized =
                explicit_bounds.iter().any(|bound| is_maybe_sized_bound(bound, renderer));
            let implicitly_sized = if *needs_sized_check && explicit_maybe_sized {
                opaque_is_implied_sized_by_use_site(renderer.tcx, *def_id)
            } else {
                false
            };

            let mut implied_bounds = implied_bounds_for_ty(
                target_ty,
                &clauses,
                implicitly_sized,
                explicit_bounds,
                *def_id,
                renderer,
            );

            let extra_bounds = implied_outlives_bounds_for_opaque(renderer.tcx, *def_id);
            if !extra_bounds.is_empty() {
                let mut seen: FxHashSet<_> = explicit_bounds.iter().cloned().collect();
                seen.extend(implied_bounds.iter().cloned());
                for bound in extra_bounds {
                    if seen.insert(bound.clone()) {
                        implied_bounds.push(bound);
                    }
                }
            }

            implied_bounds
        }
    }
}

/// Build a minimal `DocContext` for implied-bounds rendering.
///
/// This is intentionally a narrow, JSON-only shim that lets us reuse existing `clean::*`
/// conversion helpers when turning `ty::Clause` data into `rustdoc_json_types`:
/// - The implied-bounds logic starts from `ty::Clause` (param-env predicates) rather than
///   from HIR, so we don't have a preexisting clean representation to convert.
/// - The relevant clean helpers ([`crate::clean::clean_trait_ref_with_constraints`],
///   [`crate::clean::projection_to_path_segment`], [`crate::clean::clean_middle_term`],
///   [`crate::clean::clean_bound_vars`]) require a `DocContext` to access `tcx`, `param_env`, and
///   path/generic normalization logic. We don't want to duplicate them here.
///
/// This context is read-only and intentionally minimal: it only carries the fields needed by
/// the clean helpers above (e.g., `tcx`, `param_env`, `auto_traits`, and a fresh `Cache` to
/// satisfy path lookups). It does not run passes, does not mutate global caches, and does not
/// depend on the rest of the cleaning pipeline.
///
/// If this ever shows up as a hot path or becomes too heavyweight, the alternatives are:
/// - reimplement the clean logic directly in JSON and accept some duplication;
/// - move implied-bounds computation into `clean` itself, making it shared with rustdoc HTML, or
/// - refactor JSON rendering to get access to the main `DocContext`.
fn implied_bounds_doc_context<'tcx>(
    renderer: &JsonRenderer<'tcx>,
    owner_def_id: DefId,
) -> DocContext<'tcx> {
    let auto_traits = renderer
        .tcx
        .visible_traits()
        .filter(|&trait_def_id| renderer.tcx.trait_is_auto(trait_def_id))
        .collect();
    DocContext {
        tcx: renderer.tcx,
        param_env: renderer.tcx.param_env(owner_def_id),
        external_traits: Default::default(),
        active_extern_traits: Default::default(),
        args: Default::default(),
        current_type_aliases: Default::default(),
        impl_trait_bounds: Default::default(),
        generated_synthetics: Default::default(),
        auto_traits,
        cache: Cache::new(renderer.cache.document_private, renderer.cache.document_hidden),
        inlined: Default::default(),
        output_format: OutputFormat::Json,
        show_coverage: false,
    }
}

fn sized_trait_id(renderer: &JsonRenderer<'_>) -> Option<Id> {
    renderer.tcx.lang_items().sized_trait().map(|did| renderer.id_from_item_default(did.into()))
}

fn is_sized_bound(bound: &GenericBound, renderer: &JsonRenderer<'_>) -> bool {
    let Some(sized_id) = sized_trait_id(renderer) else { return false };
    matches!(
        bound,
        GenericBound::TraitBound { trait_: Path { id, .. }, modifier, .. }
            if *id == sized_id && *modifier != TraitBoundModifier::Maybe
    )
}

fn is_maybe_sized_bound(bound: &GenericBound, renderer: &JsonRenderer<'_>) -> bool {
    let Some(sized_id) = sized_trait_id(renderer) else { return false };
    matches!(
        bound,
        GenericBound::TraitBound { trait_: Path { id, .. }, modifier, .. }
            if *id == sized_id && *modifier == TraitBoundModifier::Maybe
    )
}

fn clause_targets_ty<'tcx>(clause: ty::Clause<'tcx>, target: Ty<'tcx>) -> bool {
    if let Some(trait_clause) = clause.as_trait_clause() {
        trait_clause.self_ty().skip_binder() == target
    } else if let Some(type_outlives) = clause.as_type_outlives_clause() {
        type_outlives.skip_binder().0 == target
    } else {
        false
    }
}

fn clause_to_generic_bound<'tcx>(
    clause: ty::Clause<'tcx>,
    all_clauses: &[ty::Clause<'tcx>],
    explicit_trait_bounds: &FxHashSet<(Id, TraitBoundModifier)>,
    clean_cx: &mut DocContext<'tcx>,
    renderer: &JsonRenderer<'tcx>,
) -> Option<GenericBound> {
    if let Some(trait_clause) = clause.as_trait_clause() {
        let def_id = trait_clause.def_id();
        let modifier = TraitBoundModifier::None;
        let id = renderer.id_from_item_default(def_id.into());
        if explicit_trait_bounds.contains(&(id, modifier)) {
            return None;
        }
        match renderer.tcx.as_lang_item(def_id) {
            None => {}

            // These are all the language item traits that are stable in Rust today.
            Some(
                LangItem::Sized
                | LangItem::Clone
                | LangItem::Copy
                | LangItem::Sync
                | LangItem::Drop
                | LangItem::Add
                | LangItem::Sub
                | LangItem::Mul
                | LangItem::Div
                | LangItem::Rem
                | LangItem::Neg
                | LangItem::Not
                | LangItem::BitXor
                | LangItem::BitAnd
                | LangItem::BitOr
                | LangItem::Shl
                | LangItem::Shr
                | LangItem::AddAssign
                | LangItem::SubAssign
                | LangItem::MulAssign
                | LangItem::DivAssign
                | LangItem::RemAssign
                | LangItem::BitXorAssign
                | LangItem::BitAndAssign
                | LangItem::BitOrAssign
                | LangItem::ShlAssign
                | LangItem::ShrAssign
                | LangItem::Index
                | LangItem::IndexMut
                | LangItem::Deref
                | LangItem::DerefMut
                | LangItem::Fn
                | LangItem::FnMut
                | LangItem::FnOnce
                | LangItem::AsyncFn
                | LangItem::AsyncFnMut
                | LangItem::AsyncFnOnce
                | LangItem::Iterator
                | LangItem::FusedIterator
                | LangItem::Future
                | LangItem::Unpin
                | LangItem::PartialEq
                | LangItem::PartialOrd,
            ) => {}
            Some(_) => return None,
        }

        let poly_trait_ref = trait_clause.map_bound(|pred| pred.trait_ref);
        let constraints = assoc_item_constraints_for_trait_ref(all_clauses, poly_trait_ref, clean_cx);
        let clean_path =
            clean::clean_trait_ref_with_constraints(clean_cx, poly_trait_ref, constraints);
        let path = clean_path.into_json(renderer);
        let generic_params =
            clean::clean_bound_vars(trait_clause.bound_vars(), clean_cx).into_json(renderer);

        return Some(GenericBound::TraitBound { trait_: path, generic_params, modifier });
    }

    if let Some(type_outlives) = clause.as_type_outlives_clause() {
        let ty::OutlivesPredicate(_, region) = type_outlives.skip_binder();
        if let Some(name) = region.get_name(renderer.tcx) {
            return Some(GenericBound::Outlives(name.to_string()));
        }
    }

    None
}

fn assoc_item_constraints_for_trait_ref<'tcx>(
    clauses: &[ty::Clause<'tcx>],
    poly_trait_ref: ty::Binder<'tcx, ty::TraitRef<'tcx>>,
    clean_cx: &mut DocContext<'tcx>,
) -> ThinVec<clean::AssocItemConstraint> {
    clauses
        .iter()
        .filter_map(|clause| {
            let proj_clause = clause.as_projection_clause()?;
            let proj_pred = proj_clause.skip_binder();
            let proj_trait_ref = proj_pred.projection_term.trait_ref(clean_cx.tcx);
            if !projection_applies_to_trait_ref(clean_cx.tcx, proj_trait_ref, poly_trait_ref) {
                return None;
            }
            Some(clean::AssocItemConstraint {
                assoc: clean::projection_to_path_segment(
                    proj_clause.map_bound(|pred| pred.projection_term),
                    clean_cx,
                ),
                kind: clean::AssocItemConstraintKind::Equality {
                    term: clean::clean_middle_term(
                        proj_clause.map_bound(|pred| pred.term),
                        clean_cx,
                    ),
                },
            })
        })
        .collect()
}

fn projection_applies_to_trait_ref<'tcx>(
    tcx: TyCtxt<'tcx>,
    proj_trait_ref: ty::TraitRef<'tcx>,
    trait_ref: ty::Binder<'tcx, ty::TraitRef<'tcx>>,
) -> bool {
    if proj_trait_ref == trait_ref.skip_binder() {
        return true;
    }

    let Some(fn_once_trait) = tcx.lang_items().fn_once_trait() else { return false };
    if proj_trait_ref.def_id != fn_once_trait {
        return false;
    }

    let Some(fn_trait) = tcx.lang_items().fn_trait() else { return false };
    let Some(fn_mut_trait) = tcx.lang_items().fn_mut_trait() else { return false };
    let trait_def_id = trait_ref.skip_binder().def_id;
    if trait_def_id != fn_trait && trait_def_id != fn_mut_trait {
        return false;
    }

    proj_trait_ref.args == trait_ref.skip_binder().args
}

/// Returns `true` if the function signature requires the type parameter to be `Sized`.
///
/// This includes direct uses in fn params/return values and nested positions that must be sized.
/// Uses behind indirection, or in a DST tail of a type that is itself allowed to be unsized,
/// do not require `Sized`.
fn param_requires_sized_in_fn_sig<'tcx>(
    tcx: TyCtxt<'tcx>,
    fn_def_id: DefId,
    target_param: ParamTy,
) -> bool {
    let is_function = matches!(tcx.def_kind(fn_def_id), DefKind::Fn | DefKind::AssocFn);
    if !is_function {
        return false;
    }

    let sig = tcx.fn_sig(fn_def_id).instantiate_identity();
    let sig = sig.skip_binder();
    sig.inputs().iter().any(|ty| param_requires_sized_in_ty(tcx, *ty, target_param, false))
        || param_requires_sized_in_ty(tcx, sig.output(), target_param, false)
}

fn param_requires_sized_in_ty<'tcx>(
    tcx: TyCtxt<'tcx>,
    ty: Ty<'tcx>,
    target_param: ParamTy,
    allow_unsized: bool,
) -> bool {
    match *ty.kind() {
        ty::Param(param) => param == target_param && !allow_unsized,

        // Uses behind `&`, `&mut`, `* const`, or `* mut` are not required to be `Sized`.
        ty::Ref(_, inner, _) => param_requires_sized_in_ty(tcx, inner, target_param, true),
        ty::RawPtr(inner, _) => param_requires_sized_in_ty(tcx, inner, target_param, true),

        // The types within slices and arrays must be `Sized`.
        ty::Slice(inner) => param_requires_sized_in_ty(tcx, inner, target_param, false),
        ty::Array(inner, _) => param_requires_sized_in_ty(tcx, inner, target_param, false),

        // All fields except the last field must be `Sized`.
        // The last field may be unsized if the tuple itself is a DST.
        ty::Tuple(tys) => {
            let last_index = tys.len().saturating_sub(1);
            for (index, elem) in tys.iter().enumerate() {
                let elem_allows_unsized = allow_unsized && index == last_index;
                if param_requires_sized_in_ty(tcx, elem, target_param, elem_allows_unsized) {
                    return true;
                }
            }
            false
        }

        // Structs are similar to tuples: the all fields except the last must be `Sized`,
        // while the last field may be unsized if the struct is a DST.
        ty::Adt(def, args) if def.is_struct() => {
            let variant = def.non_enum_variant();
            let last_index = variant.fields.len().saturating_sub(1);
            for (index, field) in variant.fields.iter().enumerate() {
                let field_allows_unsized = allow_unsized && index == last_index;
                let field_ty = field.ty(tcx, args);
                if param_requires_sized_in_ty(tcx, field_ty, target_param, field_allows_unsized) {
                    return true;
                }
            }
            false
        }
        ty::Pat(inner, _) => param_requires_sized_in_ty(tcx, inner, target_param, allow_unsized),
        _ => false,
    }
}

fn opaque_is_implied_sized_by_use_site(tcx: TyCtxt<'_>, opaque_def_id: DefId) -> bool {
    let Some(local_def_id) = opaque_def_id.as_local() else {
        return true;
    };
    let origin = tcx.opaque_ty_origin(local_def_id.to_def_id());
    let parent = match origin {
        OpaqueTyOrigin::FnReturn { parent, .. } | OpaqueTyOrigin::AsyncFn { parent, .. } => parent,
        OpaqueTyOrigin::TyAlias { .. } => return false,
    };
    let Some(parent_local) = parent.as_local() else {
        return true;
    };
    let Some(fn_decl) = tcx.hir_node_by_def_id(parent_local).fn_decl() else {
        return false;
    };
    let hir::FnRetTy::Return(ret_ty) = fn_decl.output else {
        return true;
    };
    match opaque_use_in_ty(ret_ty, opaque_def_id, false) {
        Some(OpaqueUse::Direct) => true,
        Some(OpaqueUse::BehindPointer) => false,
        None => true,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpaqueUse {
    Direct,
    BehindPointer,
}

fn opaque_use_in_ty<'hir, Unambig>(
    ty: &'hir hir::Ty<'hir, Unambig>,
    opaque_def_id: DefId,
    allow_unsized: bool,
) -> Option<OpaqueUse> {
    match ty.kind {
        hir::TyKind::OpaqueDef(opaque) => {
            let found = if opaque.def_id.to_def_id() == opaque_def_id {
                if !allow_unsized {
                    return Some(OpaqueUse::Direct);
                }

                Some(OpaqueUse::BehindPointer)
            } else {
                None
            };

            opaque_use_in_bounds(opaque.bounds, opaque_def_id, allow_unsized).or(found)
        }
        // References can point to DSTs, so an opaque under `&`/`&mut` does not need to be `Sized`.
        hir::TyKind::Ref(_, mut_ty) => opaque_use_in_ty(mut_ty.ty, opaque_def_id, true),
        // Raw pointers can also point to DSTs, so `* const`/`* mut` allows unsized pointees.
        hir::TyKind::Ptr(mut_ty) => opaque_use_in_ty(mut_ty.ty, opaque_def_id, true),
        // Slice elements must be `Sized`, even though the slice itself is a DST.
        hir::TyKind::Slice(inner) => opaque_use_in_ty(inner, opaque_def_id, false),
        // Array elements must be `Sized`; an unsized element would make the array ill-formed.
        hir::TyKind::Array(inner, _) => opaque_use_in_ty(inner, opaque_def_id, false),
        hir::TyKind::Tup(tys) => {
            let mut found = None;
            let last_index = tys.len().saturating_sub(1);
            for (index, ty) in tys.iter().enumerate() {
                // Only the tuple tail can be unsized; all earlier elements must be `Sized`.
                let elem_allows_unsized = allow_unsized && index == last_index;
                match opaque_use_in_ty(ty, opaque_def_id, elem_allows_unsized) {
                    Some(OpaqueUse::Direct) => return Some(OpaqueUse::Direct),
                    Some(OpaqueUse::BehindPointer) => found = Some(OpaqueUse::BehindPointer),
                    None => {}
                }
            }
            found
        }
        hir::TyKind::Path(qpath) => opaque_use_in_qpath(&qpath, opaque_def_id, allow_unsized),
        hir::TyKind::TraitObject(bounds, ..) => {
            opaque_use_in_poly_trait_refs(bounds, opaque_def_id, allow_unsized)
        }
        // `impl Trait` is not allowed inside fn pointer types, so we will not find an opaque here.
        hir::TyKind::FnPtr(_) => None,
        hir::TyKind::UnsafeBinder(unsafe_binder_ty) => {
            opaque_use_in_ty(unsafe_binder_ty.inner_ty, opaque_def_id, allow_unsized)
        }
        hir::TyKind::Pat(inner, _) => opaque_use_in_ty(inner, opaque_def_id, allow_unsized),
        _ => None,
    }
}

fn opaque_use_in_qpath<'hir>(
    qpath: &'hir hir::QPath<'hir>,
    opaque_def_id: DefId,
    allow_unsized: bool,
) -> Option<OpaqueUse> {
    match *qpath {
        hir::QPath::Resolved(self_ty, path) => {
            let mut found = None;
            if let Some(ty) = self_ty {
                match opaque_use_in_ty(ty, opaque_def_id, allow_unsized) {
                    Some(OpaqueUse::Direct) => return Some(OpaqueUse::Direct),
                    Some(OpaqueUse::BehindPointer) => found = Some(OpaqueUse::BehindPointer),
                    None => {}
                }
            }
            for segment in path.segments {
                if let Some(args) = segment.args {
                    match opaque_use_in_generic_args(args, opaque_def_id, allow_unsized) {
                        Some(OpaqueUse::Direct) => return Some(OpaqueUse::Direct),
                        Some(OpaqueUse::BehindPointer) => found = Some(OpaqueUse::BehindPointer),
                        None => {}
                    }
                }
            }
            found
        }
        hir::QPath::TypeRelative(ty, segment) => {
            let mut found = None;
            match opaque_use_in_ty(ty, opaque_def_id, allow_unsized) {
                Some(OpaqueUse::Direct) => return Some(OpaqueUse::Direct),
                Some(OpaqueUse::BehindPointer) => found = Some(OpaqueUse::BehindPointer),
                None => {}
            }
            if let Some(args) = segment.args {
                match opaque_use_in_generic_args(args, opaque_def_id, allow_unsized) {
                    Some(OpaqueUse::Direct) => return Some(OpaqueUse::Direct),
                    Some(OpaqueUse::BehindPointer) => found = Some(OpaqueUse::BehindPointer),
                    None => {}
                }
            }
            found
        }
    }
}

fn opaque_use_in_generic_args<'hir>(
    args: &'hir hir::GenericArgs<'hir>,
    opaque_def_id: DefId,
    allow_unsized: bool,
) -> Option<OpaqueUse> {
    let mut found = None;
    for arg in args.args {
        if let hir::GenericArg::Type(ty) = *arg {
            match opaque_use_in_ty(ty, opaque_def_id, allow_unsized) {
                Some(OpaqueUse::Direct) => return Some(OpaqueUse::Direct),
                Some(OpaqueUse::BehindPointer) => found = Some(OpaqueUse::BehindPointer),
                None => {}
            }
        }
    }
    for constraint in args.constraints {
        match opaque_use_in_generic_args(constraint.gen_args, opaque_def_id, allow_unsized) {
            Some(OpaqueUse::Direct) => return Some(OpaqueUse::Direct),
            Some(OpaqueUse::BehindPointer) => found = Some(OpaqueUse::BehindPointer),
            None => {}
        }
        match constraint.kind {
            hir::AssocItemConstraintKind::Equality { term } => {
                if let hir::Term::Ty(ty) = term {
                    match opaque_use_in_ty(ty, opaque_def_id, allow_unsized) {
                        Some(OpaqueUse::Direct) => return Some(OpaqueUse::Direct),
                        Some(OpaqueUse::BehindPointer) => found = Some(OpaqueUse::BehindPointer),
                        None => {}
                    }
                }
            }
            hir::AssocItemConstraintKind::Bound { bounds } => {
                match opaque_use_in_bounds(bounds, opaque_def_id, allow_unsized) {
                    Some(OpaqueUse::Direct) => return Some(OpaqueUse::Direct),
                    Some(OpaqueUse::BehindPointer) => found = Some(OpaqueUse::BehindPointer),
                    None => {}
                }
            }
        }
    }
    found
}

fn opaque_use_in_bounds<'hir>(
    bounds: &'hir [hir::GenericBound<'hir>],
    opaque_def_id: DefId,
    allow_unsized: bool,
) -> Option<OpaqueUse> {
    let mut found = None;
    for bound in bounds {
        match bound {
            hir::GenericBound::Trait(trait_ref) => {
                match opaque_use_in_path(&trait_ref.trait_ref.path, opaque_def_id, allow_unsized) {
                    Some(OpaqueUse::Direct) => return Some(OpaqueUse::Direct),
                    Some(OpaqueUse::BehindPointer) => found = Some(OpaqueUse::BehindPointer),
                    None => {}
                }
            }
            hir::GenericBound::Outlives(_) | hir::GenericBound::Use(..) => {}
        }
    }
    found
}

fn opaque_use_in_poly_trait_refs<'hir>(
    refs: &'hir [hir::PolyTraitRef<'hir>],
    opaque_def_id: DefId,
    allow_unsized: bool,
) -> Option<OpaqueUse> {
    let mut found = None;
    for trait_ref in refs {
        match opaque_use_in_path(&trait_ref.trait_ref.path, opaque_def_id, allow_unsized) {
            Some(OpaqueUse::Direct) => return Some(OpaqueUse::Direct),
            Some(OpaqueUse::BehindPointer) => found = Some(OpaqueUse::BehindPointer),
            None => {}
        }
    }
    found
}

fn opaque_use_in_path<'hir>(
    path: &'hir hir::Path<'hir>,
    opaque_def_id: DefId,
    allow_unsized: bool,
) -> Option<OpaqueUse> {
    let mut found = None;
    for segment in path.segments {
        if let Some(args) = segment.args {
            match opaque_use_in_generic_args(args, opaque_def_id, allow_unsized) {
                Some(OpaqueUse::Direct) => return Some(OpaqueUse::Direct),
                Some(OpaqueUse::BehindPointer) => found = Some(OpaqueUse::BehindPointer),
                None => {}
            }
        }
    }
    found
}

fn param_ty_for_param<'tcx>(
    tcx: TyCtxt<'tcx>,
    owner_def_id: DefId,
    param_def_id: DefId,
) -> Option<ParamTy> {
    let generics = tcx.generics_of(owner_def_id);
    let index = generics.param_def_id_to_index(tcx, param_def_id)?;
    let param_def = generics.param_at(index as usize, tcx);
    match param_def.kind {
        ty::GenericParamDefKind::Type { .. } => Some(ParamTy::for_def(param_def)),
        _ => None,
    }
}

fn implied_outlives_bounds_for_param<'tcx>(
    tcx: TyCtxt<'tcx>,
    owner_def_id: DefId,
    param_ty: ParamTy,
) -> Vec<GenericBound> {
    let Some(local_def_id) = owner_def_id.as_local() else {
        return Vec::new();
    };

    let assumed_wf = tcx.assumed_wf_types(local_def_id);
    if assumed_wf.is_empty() {
        return Vec::new();
    }

    let param_env = tcx.param_env(owner_def_id);
    let infcx = tcx.infer_ctxt().build(TypingMode::non_body_analysis());
    let env = OutlivesEnvironment::new(
        &infcx,
        local_def_id,
        param_env,
        assumed_wf.iter().map(|(ty, _)| *ty),
    );

    env.region_bound_pairs()
        .iter()
        .filter_map(|predicate| match predicate {
            ty::OutlivesPredicate(GenericKind::Param(param), region)
                if param.index == param_ty.index =>
            {
                region.get_name(tcx).map(|name| GenericBound::Outlives(name.to_string()))
            }
            _ => None,
        })
        .collect()
}

/// If the opaque is a TAIT / ATPIT, return any additional outlives bounds.
///
/// For example, `type Foo<'a, T> = &'a impl PartialEq<T>;`
/// has an implied `+ 'a` bound that would be returned here.
///
/// If this function is called with a different kind of opaque, it returns no bounds.
///
/// We also aren't able to return any bounds for cross-crate TAITs due to missing metadata.
fn implied_outlives_bounds_for_opaque<'tcx>(
    tcx: TyCtxt<'tcx>,
    opaque_def_id: DefId,
) -> Vec<GenericBound> {
    if tcx.def_kind(opaque_def_id) != DefKind::OpaqueTy {
        return Vec::new();
    }

    let Some(local_def_id) = opaque_def_id.as_local() else {
        // Cross-crate TAITs don't carry the parent WF info in metadata,
        // so we can't infer outlives bounds here.
        // FIXME: Get metadata on extern opaques, then make this precise.
        return Vec::new();
    };

    let OpaqueTyOrigin::TyAlias { parent, .. } = tcx.opaque_ty_origin(local_def_id.to_def_id())
    else {
        return Vec::new();
    };

    let Some(local_parent) = parent.as_local() else {
        // Cross-crate TAITs don't carry the parent WF info in metadata,
        // so we can't infer outlives bounds here.
        return Vec::new();
    };

    let param_env = tcx.param_env(parent);
    let parent_ty = tcx.type_of(parent).instantiate_identity();
    let infcx = tcx.infer_ctxt().build(TypingMode::non_body_analysis());
    let env = OutlivesEnvironment::new(&infcx, local_parent, param_env, [parent_ty]);

    let target_args = ty::GenericArgs::identity_for_item(tcx, parent).extend_to(
        tcx,
        opaque_def_id,
        |param, _| tcx.map_opaque_lifetime_to_parent_lifetime(param.def_id.expect_local()).into(),
    );
    let target_alias = AliasTy::new_from_args(tcx, opaque_def_id, target_args);
    env.region_bound_pairs()
        .iter()
        .filter_map(|predicate| match predicate {
            ty::OutlivesPredicate(GenericKind::Alias(alias), region) if *alias == target_alias => {
                region.get_name(tcx).map(|name| GenericBound::Outlives(name.to_string()))
            }
            _ => None,
        })
        .collect()
}
