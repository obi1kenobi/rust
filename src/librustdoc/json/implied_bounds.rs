use rustc_data_structures::fx::FxHashSet;
use rustc_hir::LangItem;
use rustc_hir::def::DefKind;
use rustc_hir::def_id::DefId;
use rustc_infer::infer::region_constraints::GenericKind;
use rustc_middle::ty::{self, AliasTy, ParamTy, Ty, TyCtxt, TypingMode};
use rustc_trait_selection::infer::TyCtxtInferExt;
use rustc_trait_selection::infer::outlives::env::OutlivesEnvironment;
use rustc_trait_selection::regions::OutlivesEnvironmentBuildExt;
use rustdoc_json_types::{GenericBound, Id, Path, TraitBoundModifier};

use crate::clean;
use crate::json::JsonRenderer;

pub(crate) fn sized_trait_id(renderer: &JsonRenderer<'_>) -> Option<Id> {
    renderer.tcx.lang_items().sized_trait().map(|did| renderer.id_from_item_default(did.into()))
}

pub(crate) fn is_sized_bound(bound: &GenericBound, renderer: &JsonRenderer<'_>) -> bool {
    let Some(sized_id) = sized_trait_id(renderer) else { return false };
    matches!(
        bound,
        GenericBound::TraitBound { trait_: Path { id, .. }, modifier, .. }
            if *id == sized_id && *modifier != TraitBoundModifier::Maybe
    )
}

pub(crate) fn is_maybe_sized_bound(bound: &GenericBound, renderer: &JsonRenderer<'_>) -> bool {
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

pub(crate) fn clause_to_generic_bound<'tcx>(
    clause: ty::Clause<'tcx>,
    explicit_trait_bounds: &FxHashSet<(Id, TraitBoundModifier)>,
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

        let path = Path { path: renderer.tcx.item_name(def_id).to_string(), id, args: None };

        return Some(GenericBound::TraitBound {
            trait_: path,
            generic_params: Vec::new(),
            modifier,
        });
    }

    if let Some(type_outlives) = clause.as_type_outlives_clause() {
        let ty::OutlivesPredicate(_, region) = type_outlives.skip_binder();
        if let Some(name) = region.get_name(renderer.tcx) {
            return Some(GenericBound::Outlives(name.to_string()));
        }
    }

    None
}

pub(crate) fn compute_implied_bounds_for_ty<'tcx>(
    target_ty: Ty<'tcx>,
    clauses: &[ty::Clause<'tcx>],
    implicitly_sized: bool,
    explicit_bounds: &[GenericBound],
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

    let mut added_sized_bound = false;
    for clause in clauses {
        if !clause_targets_ty(*clause, target_ty) {
            continue;
        }

        if let Some(bound) = clause_to_generic_bound(*clause, &explicit_trait_bounds, renderer) {
            if is_sized_bound(&bound, renderer) {
                added_sized_bound = true;
            }

            if seen.insert(bound.clone()) {
                implied_bounds.push(bound);
            }
        }
    }

    if implicitly_sized && !added_sized_bound {
        if let (Some(sized_id), Some(sized_def_id)) =
            (sized_trait_id(renderer), renderer.tcx.lang_items().sized_trait())
        {
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

fn fn_param_used_directly<'tcx>(
    tcx: TyCtxt<'tcx>,
    owner_def_id: DefId,
    target_ty: Ty<'tcx>,
) -> bool {
    let is_function = matches!(tcx.def_kind(owner_def_id), DefKind::Fn | DefKind::AssocFn);
    if !is_function {
        return false;
    }

    let sig = tcx.fn_sig(owner_def_id).instantiate_identity();
    let sig = sig.skip_binder();
    sig.inputs().iter().any(|ty| *ty == target_ty) || sig.output() == target_ty
}

fn param_ty_for_param<'tcx>(
    tcx: TyCtxt<'tcx>,
    owner_def_id: DefId,
    param_def_id: DefId,
) -> Option<Ty<'tcx>> {
    let generics = tcx.generics_of(owner_def_id);
    let index = generics.param_def_id_to_index(tcx, param_def_id)?;
    let param_def = generics.param_at(index as usize, tcx);
    match param_def.kind {
        ty::GenericParamDefKind::Type { .. } => Some(ParamTy::for_def(param_def).to_ty(tcx)),
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

pub(crate) fn implied_bounds_for_type_param<'tcx>(
    owner_def_id: DefId,
    param_def_id: DefId,
    explicit_bounds: &[GenericBound],
    renderer: &JsonRenderer<'tcx>,
) -> Vec<GenericBound> {
    let Some(target_ty) = param_ty_for_param(renderer.tcx, owner_def_id, param_def_id) else {
        return Vec::new();
    };

    let clauses = renderer.tcx.param_env(owner_def_id).caller_bounds();
    let allows_unsized = explicit_bounds.iter().any(|bound| is_maybe_sized_bound(bound, renderer));
    let implicitly_sized =
        !allows_unsized || fn_param_used_directly(renderer.tcx, owner_def_id, target_ty);

    let mut implied_bounds = compute_implied_bounds_for_ty(
        target_ty,
        clauses.as_slice(),
        implicitly_sized,
        explicit_bounds,
        renderer,
    );

    let ty::Param(param_ty) = target_ty.kind() else {
        return implied_bounds;
    };

    let extra_bounds = implied_outlives_bounds_for_param(renderer.tcx, owner_def_id, *param_ty);
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
    compute_implied_bounds_for_ty(target_ty, &clauses, !allows_unsized, explicit_bounds, renderer)
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
        clean::ImplTraitOrigin::Opaque { def_id, forced_sized } => {
            let args = ty::GenericArgs::identity_for_item(renderer.tcx, *def_id);
            let target_ty = Ty::new_alias(
                renderer.tcx,
                ty::Opaque,
                AliasTy::new_from_args(renderer.tcx, *def_id, args),
            );
            let clauses = renderer.tcx.item_bounds(*def_id).instantiate(renderer.tcx, args);
            compute_implied_bounds_for_ty(
                target_ty,
                &clauses,
                *forced_sized,
                explicit_bounds,
                renderer,
            )
        }
    }
}
