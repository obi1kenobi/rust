//! These from impls are used to create the JSON types which get serialized. They're very close to
//! the `clean` types but with some fields removed or stringified to simplify the output and not
//! expose unstable compiler internals.

use rustc_abi::ExternAbi;
use rustc_ast::ast;
use rustc_data_structures::fx::{FxHashMap, FxHashSet};
use rustc_data_structures::thin_vec::ThinVec;
use rustc_hir as hir;
use rustc_hir::attrs::{self, DeprecatedSince, DocAttribute, DocInline, HideOrShow};
use rustc_hir::def::{CtorKind, DefKind};
use rustc_hir::def_id::DefId;
use rustc_hir::{HeaderSafety, LangItem, Safety};
use rustc_infer::infer::region_constraints::GenericKind;
use rustc_metadata::rendered_const;
use rustc_middle::ty::TyCtxt;
use rustc_middle::bug;
use rustc_middle::ty::{self, AliasTy, ParamTy, Ty, TyCtxt, TypingMode};
use rustc_span::{Pos, Symbol, kw, sym};
use rustc_trait_selection::infer::TyCtxtInferExt;
use rustc_trait_selection::infer::outlives::env::OutlivesEnvironment;
use rustc_trait_selection::regions::OutlivesEnvironmentBuildExt;
use rustdoc_json_types::*;

use crate::clean::{self, ItemId};
use crate::formats::item_type::ItemType;
use crate::json::JsonRenderer;
use crate::passes::collect_intra_doc_links::UrlFragment;

impl<'tcx> JsonRenderer<'tcx> {
    pub(super) fn convert_item(&self, item: &clean::Item) -> Option<Item> {
        let deprecation = item.deprecation(self.tcx);
        let links = self
            .cache
            .intra_doc_links
            .get(&item.item_id)
            .into_iter()
            .flatten()
            .map(|clean::ItemLink { link, page_id, fragment, .. }| {
                let id = match fragment {
                    Some(UrlFragment::Item(frag_id)) => *frag_id,
                    // FIXME: Pass the `UserWritten` segment to JSON consumer.
                    Some(UrlFragment::UserWritten(_)) | None => *page_id,
                };

                (String::from(&**link), self.id_from_item_default(id.into()))
            })
            .collect();
        let docs = item.opt_doc_value();
        let attrs = item
            .attrs
            .other_attrs
            .iter()
            .flat_map(|a| maybe_from_hir_attr(a, item.item_id, self.tcx))
            .collect();
        let span = item.span(self.tcx);
        let visibility = item.visibility(self.tcx);
        let clean::ItemInner { name, item_id, .. } = *item.inner;
        let id = self.id_from_item(item);
        let inner = match item.kind {
            clean::KeywordItem | clean::AttributeItem => return None,
            clean::StrippedItem(ref inner) => {
                match &**inner {
                    // We document stripped modules as with `Module::is_stripped` set to
                    // `true`, to prevent contained items from being orphaned for downstream users,
                    // as JSON does no inlining.
                    clean::ModuleItem(_)
                        if self.imported_items.contains(&item_id.expect_def_id()) =>
                    {
                        from_clean_item(item, self)
                    }
                    _ => return None,
                }
            }
            _ => from_clean_item(item, self),
        };
        Some(Item {
            id,
            crate_id: item_id.krate().as_u32(),
            name: name.map(|sym| sym.to_string()),
            span: span.and_then(|span| span.into_json(self)),
            visibility: visibility.into_json(self),
            docs,
            attrs,
            deprecation: deprecation.into_json(self),
            inner,
            links,
        })
    }

    fn ids(&self, items: &[clean::Item]) -> Vec<Id> {
        items
            .iter()
            .filter(|i| !i.is_stripped() && !i.is_keyword() && !i.is_attribute())
            .map(|i| self.id_from_item(i))
            .collect()
    }

    fn ids_keeping_stripped(&self, items: &[clean::Item]) -> Vec<Option<Id>> {
        items
            .iter()
            .map(|i| {
                (!i.is_stripped() && !i.is_keyword() && !i.is_attribute())
                    .then(|| self.id_from_item(i))
            })
            .collect()
    }

    fn sized_trait_id(&self) -> Option<Id> {
        self.tcx.lang_items().sized_trait().map(|did| self.id_from_item_default(did.into()))
    }

    fn is_sized_bound(&self, bound: &GenericBound) -> bool {
        let Some(sized_id) = self.sized_trait_id() else { return false };
        matches!(
            bound,
            GenericBound::TraitBound { trait_: Path { id, .. }, modifier, .. }
                if *id == sized_id && *modifier != TraitBoundModifier::Maybe
        )
    }

    fn is_maybe_sized_bound(&self, bound: &GenericBound) -> bool {
        let Some(sized_id) = self.sized_trait_id() else { return false };
        matches!(
            bound,
            GenericBound::TraitBound { trait_: Path { id, .. }, modifier, .. }
                if *id == sized_id && *modifier == TraitBoundModifier::Maybe
        )
    }

    fn clause_targets_ty(&self, clause: ty::Clause<'tcx>, target: Ty<'tcx>) -> bool {
        if let Some(trait_clause) = clause.as_trait_clause() {
            trait_clause.self_ty().skip_binder() == target
        } else if let Some(type_outlives) = clause.as_type_outlives_clause() {
            type_outlives.skip_binder().0 == target
        } else {
            false
        }
    }

    fn clause_to_generic_bound(
        &self,
        clause: ty::Clause<'tcx>,
        explicit_trait_bounds: &FxHashSet<(Id, TraitBoundModifier)>,
    ) -> Option<GenericBound> {
        if let Some(trait_clause) = clause.as_trait_clause() {
            let def_id = trait_clause.def_id();
            let modifier = TraitBoundModifier::None;
            let id = self.id_from_item_default(def_id.into());
            if explicit_trait_bounds.contains(&(id, modifier)) {
                return None;
            }
            match self.tcx.as_lang_item(def_id) {
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

            let path = Path { path: self.tcx.item_name(def_id).to_string(), id, args: None };

            return Some(GenericBound::TraitBound {
                trait_: path,
                generic_params: Vec::new(),
                modifier,
            });
        }

        if let Some(type_outlives) = clause.as_type_outlives_clause() {
            let ty::OutlivesPredicate(_, region) = type_outlives.skip_binder();
            if let Some(name) = region.get_name(self.tcx) {
                return Some(GenericBound::Outlives(name.to_string()));
            }
        }

        None
    }

    fn compute_implied_bounds_for_ty(
        &self,
        target_ty: Ty<'tcx>,
        clauses: &[ty::Clause<'tcx>],
        implicitly_sized: bool,
        explicit_bounds: &[GenericBound],
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
            if !self.clause_targets_ty(*clause, target_ty) {
                continue;
            }

            if let Some(bound) = self.clause_to_generic_bound(*clause, &explicit_trait_bounds) {
                if self.is_sized_bound(&bound) {
                    added_sized_bound = true;
                }

                if seen.insert(bound.clone()) {
                    implied_bounds.push(bound);
                }
            }
        }

        if implicitly_sized && !added_sized_bound {
            if let (Some(sized_id), Some(sized_def_id)) =
                (self.sized_trait_id(), self.tcx.lang_items().sized_trait())
            {
                let sized_bound = GenericBound::TraitBound {
                    trait_: Path {
                        path: self.tcx.item_name(sized_def_id).to_string(),
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

    fn fn_param_used_directly(&self, owner_def_id: DefId, target_ty: Ty<'tcx>) -> bool {
        let is_function = matches!(self.tcx.def_kind(owner_def_id), DefKind::Fn | DefKind::AssocFn);
        if !is_function {
            return false;
        }

        let sig = self.tcx.fn_sig(owner_def_id).instantiate_identity();
        let sig = sig.skip_binder();
        sig.inputs().iter().any(|ty| *ty == target_ty) || sig.output() == target_ty
    }

    fn param_ty_for_param(&self, owner_def_id: DefId, param_def_id: DefId) -> Option<Ty<'tcx>> {
        let generics = self.tcx.generics_of(owner_def_id);
        let index = generics.param_def_id_to_index(self.tcx, param_def_id)?;
        let param_def = generics.param_at(index as usize, self.tcx);
        match param_def.kind {
            ty::GenericParamDefKind::Type { .. } => {
                Some(ParamTy::for_def(param_def).to_ty(self.tcx))
            }
            _ => None,
        }
    }

    fn implied_outlives_bounds_for_param(
        &self,
        owner_def_id: DefId,
        param_ty: ParamTy,
    ) -> Vec<GenericBound> {
        let Some(local_def_id) = owner_def_id.as_local() else {
            return Vec::new();
        };

        let assumed_wf = self.tcx.assumed_wf_types(local_def_id);
        if assumed_wf.is_empty() {
            return Vec::new();
        }

        let param_env = self.tcx.param_env(owner_def_id);
        let infcx = self.tcx.infer_ctxt().build(TypingMode::non_body_analysis());
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
                    region.get_name(self.tcx).map(|name| GenericBound::Outlives(name.to_string()))
                }
                _ => None,
            })
            .collect()
    }

    fn implied_bounds_for_type_param(
        &self,
        owner_def_id: DefId,
        param_def_id: DefId,
        explicit_bounds: &[GenericBound],
    ) -> Vec<GenericBound> {
        let Some(target_ty) = self.param_ty_for_param(owner_def_id, param_def_id) else {
            return Vec::new();
        };

        let clauses = self.tcx.param_env(owner_def_id).caller_bounds();
        let allows_unsized = explicit_bounds.iter().any(|bound| self.is_maybe_sized_bound(bound));
        let implicitly_sized =
            !allows_unsized || self.fn_param_used_directly(owner_def_id, target_ty);

        let mut implied_bounds = self.compute_implied_bounds_for_ty(
            target_ty,
            clauses.as_slice(),
            implicitly_sized,
            explicit_bounds,
        );

        let ty::Param(param_ty) = target_ty.kind() else {
            return implied_bounds;
        };

        let extra_bounds = self.implied_outlives_bounds_for_param(owner_def_id, *param_ty);
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

    fn implied_bounds_for_assoc_type(
        &self,
        assoc_def_id: DefId,
        explicit_bounds: &[GenericBound],
    ) -> Vec<GenericBound> {
        let assoc_item = self.tcx.associated_item(assoc_def_id);
        if !matches!(assoc_item.container, ty::AssocContainer::Trait) {
            return Vec::new();
        }

        let args = ty::GenericArgs::identity_for_item(self.tcx, assoc_def_id);
        let target_ty = Ty::new_alias(
            self.tcx,
            ty::Projection,
            AliasTy::new_from_args(self.tcx, assoc_def_id, args),
        );
        let clauses = self.tcx.item_bounds(assoc_def_id).instantiate(self.tcx, args);
        let allows_unsized = explicit_bounds.iter().any(|bound| self.is_maybe_sized_bound(bound));
        self.compute_implied_bounds_for_ty(target_ty, &clauses, !allows_unsized, explicit_bounds)
    }

    fn implied_bounds_for_impl_trait(
        &self,
        origin: &clean::ImplTraitOrigin,
        explicit_bounds: &[GenericBound],
    ) -> Vec<GenericBound> {
        match origin {
            clean::ImplTraitOrigin::Param { def_id } => {
                let Some(owner_def_id) = self.tcx.opt_parent(*def_id) else { return Vec::new() };
                self.implied_bounds_for_type_param(owner_def_id, *def_id, explicit_bounds)
            }
            clean::ImplTraitOrigin::Opaque { def_id, forced_sized } => {
                let args = ty::GenericArgs::identity_for_item(self.tcx, *def_id);
                let target_ty = Ty::new_alias(
                    self.tcx,
                    ty::Opaque,
                    AliasTy::new_from_args(self.tcx, *def_id, args),
                );
                let clauses = self.tcx.item_bounds(*def_id).instantiate(self.tcx, args);
                self.compute_implied_bounds_for_ty(
                    target_ty,
                    &clauses,
                    *forced_sized,
                    explicit_bounds,
                )
            }
        }
    }

    fn generics_into_json(&self, generics: &clean::Generics, owner_def_id: DefId) -> Generics {
        let mut param_by_name = FxHashMap::default();
        let mut param_bounds: FxHashMap<_, Vec<GenericBound>> = FxHashMap::default();
        let mut explicit_bounds: FxHashMap<_, Vec<GenericBound>> = FxHashMap::default();

        for param in &generics.params {
            if let clean::GenericParamDefKind::Type { bounds, .. } = &param.kind {
                let bounds_json: Vec<GenericBound> = bounds.into_json(self);
                param_by_name.insert(param.name, param.def_id);
                explicit_bounds.entry(param.def_id).or_default().extend(bounds_json.clone());
                param_bounds.insert(param.def_id, bounds_json);
            }
        }

        for predicate in &generics.where_predicates {
            if let clean::WherePredicate::BoundPredicate {
                ty: clean::Type::Generic(name),
                bounds,
                ..
            } = predicate
                && let Some(def_id) = param_by_name.get(name)
            {
                let where_bounds: Vec<GenericBound> = bounds.into_json(self);
                explicit_bounds.entry(*def_id).or_default().extend(where_bounds);
            }
        }

        let params = generics
            .params
            .iter()
            .map(|param| match &param.kind {
                clean::GenericParamDefKind::Lifetime { outlives } => GenericParamDef {
                    name: param.name.to_string(),
                    kind: GenericParamDefKind::Lifetime { outlives: outlives.into_json(self) },
                },
                clean::GenericParamDefKind::Type { bounds, default, synthetic } => {
                    let bounds_json = param_bounds
                        .remove(&param.def_id)
                        .unwrap_or_else(|| bounds.into_json(self));
                    let explicit_bounds = explicit_bounds
                        .remove(&param.def_id)
                        .unwrap_or_else(|| bounds_json.clone());
                    let implied_bounds = self.implied_bounds_for_type_param(
                        owner_def_id,
                        param.def_id,
                        &explicit_bounds,
                    );
                    GenericParamDef {
                        name: param.name.to_string(),
                        kind: GenericParamDefKind::Type {
                            bounds: bounds_json,
                            implied_bounds,
                            default: default.into_json(self),
                            is_synthetic: *synthetic,
                        },
                    }
                }
                clean::GenericParamDefKind::Const { ty, default } => GenericParamDef {
                    name: param.name.to_string(),
                    kind: GenericParamDefKind::Const {
                        type_: ty.into_json(self),
                        default: default.as_ref().map(|x| x.as_ref().clone()),
                    },
                },
            })
            .collect();

        Generics { params, where_predicates: generics.where_predicates.into_json(self) }
    }
}

pub(crate) trait FromClean<T> {
    fn from_clean(f: &T, renderer: &JsonRenderer<'_>) -> Self;
}

pub(crate) trait IntoJson<T> {
    fn into_json(&self, renderer: &JsonRenderer<'_>) -> T;
}

impl<T, U> IntoJson<U> for T
where
    U: FromClean<T>,
{
    fn into_json(&self, renderer: &JsonRenderer<'_>) -> U {
        U::from_clean(self, renderer)
    }
}

impl<T, U> FromClean<Box<T>> for U
where
    U: FromClean<T>,
{
    fn from_clean(opt: &Box<T>, renderer: &JsonRenderer<'_>) -> Self {
        opt.as_ref().into_json(renderer)
    }
}

impl<T, U> FromClean<Option<T>> for Option<U>
where
    U: FromClean<T>,
{
    fn from_clean(opt: &Option<T>, renderer: &JsonRenderer<'_>) -> Self {
        opt.as_ref().map(|x| x.into_json(renderer))
    }
}

impl<T, U> FromClean<Vec<T>> for Vec<U>
where
    U: FromClean<T>,
{
    fn from_clean(items: &Vec<T>, renderer: &JsonRenderer<'_>) -> Self {
        items.iter().map(|i| i.into_json(renderer)).collect()
    }
}

impl<T, U> FromClean<ThinVec<T>> for Vec<U>
where
    U: FromClean<T>,
{
    fn from_clean(items: &ThinVec<T>, renderer: &JsonRenderer<'_>) -> Self {
        items.iter().map(|i| i.into_json(renderer)).collect()
    }
}

impl FromClean<clean::Span> for Option<Span> {
    fn from_clean(span: &clean::Span, renderer: &JsonRenderer<'_>) -> Self {
        match span.filename(renderer.sess()) {
            rustc_span::FileName::Real(name) => {
                if let Some(local_path) = name.into_local_path() {
                    let hi = span.hi(renderer.sess());
                    let lo = span.lo(renderer.sess());
                    Some(Span {
                        filename: local_path,
                        begin: (lo.line, lo.col.to_usize() + 1),
                        end: (hi.line, hi.col.to_usize() + 1),
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl FromClean<Option<ty::Visibility<DefId>>> for Visibility {
    fn from_clean(v: &Option<ty::Visibility<DefId>>, renderer: &JsonRenderer<'_>) -> Self {
        match v {
            None => Visibility::Default,
            Some(ty::Visibility::Public) => Visibility::Public,
            Some(ty::Visibility::Restricted(did)) if did.is_crate_root() => Visibility::Crate,
            Some(ty::Visibility::Restricted(did)) => Visibility::Restricted {
                parent: renderer.id_from_item_default((*did).into()),
                path: renderer.tcx.def_path(*did).to_string_no_crate_verbose(),
            },
        }
    }
}

impl FromClean<attrs::Deprecation> for Deprecation {
    fn from_clean(deprecation: &attrs::Deprecation, _renderer: &JsonRenderer<'_>) -> Self {
        let attrs::Deprecation { since, note, suggestion: _ } = deprecation;
        let since = match since {
            DeprecatedSince::RustcVersion(version) => Some(version.to_string()),
            DeprecatedSince::Future => Some("TBD".to_string()),
            DeprecatedSince::NonStandard(since) => Some(since.to_string()),
            DeprecatedSince::Unspecified | DeprecatedSince::Err => None,
        };
        Deprecation { since, note: note.map(|sym| sym.to_string()) }
    }
}

impl FromClean<clean::GenericArgs> for Option<Box<GenericArgs>> {
    fn from_clean(generic_args: &clean::GenericArgs, renderer: &JsonRenderer<'_>) -> Self {
        use clean::GenericArgs::*;
        match generic_args {
            AngleBracketed { args, constraints } => {
                if generic_args.is_empty() {
                    None
                } else {
                    Some(Box::new(GenericArgs::AngleBracketed {
                        args: args.into_json(renderer),
                        constraints: constraints.into_json(renderer),
                    }))
                }
            }
            Parenthesized { inputs, output } => Some(Box::new(GenericArgs::Parenthesized {
                inputs: inputs.into_json(renderer),
                output: output.into_json(renderer),
            })),
            ReturnTypeNotation => Some(Box::new(GenericArgs::ReturnTypeNotation)),
        }
    }
}

impl FromClean<clean::GenericArg> for GenericArg {
    fn from_clean(arg: &clean::GenericArg, renderer: &JsonRenderer<'_>) -> Self {
        use clean::GenericArg::*;
        match arg {
            Lifetime(l) => GenericArg::Lifetime(l.into_json(renderer)),
            Type(t) => GenericArg::Type(t.into_json(renderer)),
            Const(box c) => GenericArg::Const(c.into_json(renderer)),
            Infer => GenericArg::Infer,
        }
    }
}

impl FromClean<clean::ConstantKind> for Constant {
    // FIXME(generic_const_items): Add support for generic const items.
    fn from_clean(constant: &clean::ConstantKind, renderer: &JsonRenderer<'_>) -> Self {
        let tcx = renderer.tcx;
        let expr = constant.expr(tcx);
        let value = constant.value(tcx);
        let is_literal = constant.is_literal(tcx);
        Constant { expr, value, is_literal }
    }
}

impl FromClean<clean::AssocItemConstraint> for AssocItemConstraint {
    fn from_clean(constraint: &clean::AssocItemConstraint, renderer: &JsonRenderer<'_>) -> Self {
        AssocItemConstraint {
            name: constraint.assoc.name.to_string(),
            args: constraint.assoc.args.into_json(renderer),
            binding: constraint.kind.into_json(renderer),
        }
    }
}

impl FromClean<clean::AssocItemConstraintKind> for AssocItemConstraintKind {
    fn from_clean(kind: &clean::AssocItemConstraintKind, renderer: &JsonRenderer<'_>) -> Self {
        use clean::AssocItemConstraintKind::*;
        match kind {
            Equality { term } => AssocItemConstraintKind::Equality(term.into_json(renderer)),
            Bound { bounds } => AssocItemConstraintKind::Constraint(bounds.into_json(renderer)),
        }
    }
}

fn from_clean_item(item: &clean::Item, renderer: &JsonRenderer<'_>) -> ItemEnum {
    use clean::ItemKind::*;
    let name = item.name;
    let is_crate = item.is_crate();
    let header = item.fn_header(renderer.tcx);
    let owner_def_id = match item.item_id {
        ItemId::DefId(did) => did,
        ItemId::Auto { trait_, .. } => trait_,
        ItemId::Blanket { impl_id, .. } => impl_id,
    };

    match &item.inner.kind {
        ModuleItem(m) => {
            ItemEnum::Module(Module { is_crate, items: renderer.ids(&m.items), is_stripped: false })
        }
        ImportItem(i) => ItemEnum::Use(i.into_json(renderer)),
        StructItem(s) => {
            let has_stripped_fields = s.has_stripped_entries();
            let kind = match s.ctor_kind {
                Some(CtorKind::Fn) => StructKind::Tuple(renderer.ids_keeping_stripped(&s.fields)),
                Some(CtorKind::Const) => {
                    assert!(s.fields.is_empty());
                    StructKind::Unit
                }
                None => StructKind::Plain { fields: renderer.ids(&s.fields), has_stripped_fields },
            };

            ItemEnum::Struct(Struct {
                kind,
                generics: renderer.generics_into_json(&s.generics, owner_def_id),
                impls: Vec::new(),
            })
        }
        UnionItem(u) => {
            let has_stripped_fields = u.has_stripped_entries();
            ItemEnum::Union(Union {
                generics: renderer.generics_into_json(&u.generics, owner_def_id),
                has_stripped_fields,
                fields: renderer.ids(&u.fields),
                impls: Vec::new(),
            })
        }
        StructFieldItem(f) => ItemEnum::StructField(f.into_json(renderer)),
        EnumItem(e) => {
            let has_stripped_variants = e.has_stripped_entries();
            ItemEnum::Enum(Enum {
                generics: renderer.generics_into_json(&e.generics, owner_def_id),
                has_stripped_variants,
                variants: renderer.ids(&e.variants.as_slice().raw),
                impls: Vec::new(),
            })
        }
        VariantItem(v) => ItemEnum::Variant(v.into_json(renderer)),
        FunctionItem(f) => ItemEnum::Function(from_clean_function(
            f,
            owner_def_id,
            true,
            header.unwrap(),
            renderer,
        )),
        ForeignFunctionItem(f, _) => ItemEnum::Function(from_clean_function(
            f,
            owner_def_id,
            false,
            header.unwrap(),
            renderer,
        )),
        TraitItem(t) => ItemEnum::Trait(t.into_json(renderer)),
        TraitAliasItem(t) => ItemEnum::TraitAlias(TraitAlias {
            generics: renderer.generics_into_json(&t.generics, owner_def_id),
            params: t.bounds.into_json(renderer),
        }),
        MethodItem(m, _) => ItemEnum::Function(from_clean_function(
            m,
            owner_def_id,
            true,
            header.unwrap(),
            renderer,
        )),
        RequiredMethodItem(m) => ItemEnum::Function(from_clean_function(
            m,
            owner_def_id,
            false,
            header.unwrap(),
            renderer,
        )),
        ImplItem(i) => ItemEnum::Impl(from_clean_impl(i, owner_def_id, renderer)),
        StaticItem(s) => ItemEnum::Static(from_clean_static(s, rustc_hir::Safety::Safe, renderer)),
        ForeignStaticItem(s, safety) => ItemEnum::Static(from_clean_static(s, *safety, renderer)),
        ForeignTypeItem => ItemEnum::ExternType,
        TypeAliasItem(t) => ItemEnum::TypeAlias(TypeAlias {
            type_: t.type_.into_json(renderer),
            generics: renderer.generics_into_json(&t.generics, owner_def_id),
        }),
        // FIXME(generic_const_items): Add support for generic free consts
        ConstantItem(ci) => ItemEnum::Constant {
            type_: ci.type_.into_json(renderer),
            const_: ci.kind.into_json(renderer),
        },
        MacroItem(m) => ItemEnum::Macro(m.source.clone()),
        ProcMacroItem(m) => ItemEnum::ProcMacro(m.into_json(renderer)),
        PrimitiveItem(p) => {
            ItemEnum::Primitive(Primitive {
                name: p.as_sym().to_string(),
                impls: Vec::new(), // Added in JsonRenderer::item
            })
        }
        // FIXME(generic_const_items): Add support for generic associated consts.
        RequiredAssocConstItem(_generics, ty) => {
            ItemEnum::AssocConst { type_: ty.into_json(renderer), value: None }
        }
        // FIXME(generic_const_items): Add support for generic associated consts.
        ProvidedAssocConstItem(ci) | ImplAssocConstItem(ci) => ItemEnum::AssocConst {
            type_: ci.type_.into_json(renderer),
            value: Some(ci.kind.expr(renderer.tcx)),
        },
        RequiredAssocTypeItem(generics, bounds) => {
            let bounds_json: Vec<GenericBound> = bounds.into_json(renderer);
            ItemEnum::AssocType {
                generics: renderer.generics_into_json(generics, owner_def_id),
                bounds: bounds_json.clone(),
                implied_bounds: renderer.implied_bounds_for_assoc_type(owner_def_id, &bounds_json),
                type_: None,
            }
        }
        AssocTypeItem(ty, bounds) => {
            let bounds_json: Vec<GenericBound> = bounds.into_json(renderer);
            ItemEnum::AssocType {
                generics: renderer.generics_into_json(&ty.generics, owner_def_id),
                bounds: bounds_json.clone(),
                implied_bounds: renderer.implied_bounds_for_assoc_type(owner_def_id, &bounds_json),
                type_: Some(ty.item_type.as_ref().unwrap_or(&ty.type_).into_json(renderer)),
            }
        }
        // `convert_item` early returns `None` for stripped items, keywords and attributes.
        KeywordItem | AttributeItem => unreachable!(),
        StrippedItem(inner) => {
            match inner.as_ref() {
                ModuleItem(m) => ItemEnum::Module(Module {
                    is_crate,
                    items: renderer.ids(&m.items),
                    is_stripped: true,
                }),
                // `convert_item` early returns `None` for stripped items we're not including
                _ => unreachable!(),
            }
        }
        ExternCrateItem { src } => ItemEnum::ExternCrate {
            name: name.as_ref().unwrap().to_string(),
            rename: src.map(|x| x.to_string()),
        },
    }
}

impl FromClean<clean::Struct> for Struct {
    fn from_clean(struct_: &clean::Struct, renderer: &JsonRenderer<'_>) -> Self {
        let has_stripped_fields = struct_.has_stripped_entries();
        let clean::Struct { ctor_kind, generics, fields } = struct_;

        let kind = match ctor_kind {
            Some(CtorKind::Fn) => StructKind::Tuple(renderer.ids_keeping_stripped(fields)),
            Some(CtorKind::Const) => {
                assert!(fields.is_empty());
                StructKind::Unit
            }
            None => StructKind::Plain { fields: renderer.ids(fields), has_stripped_fields },
        };

        Struct {
            kind,
            generics: generics.into_json(renderer),
            impls: Vec::new(), // Added in JsonRenderer::item
        }
    }
}

impl FromClean<clean::Union> for Union {
    fn from_clean(union_: &clean::Union, renderer: &JsonRenderer<'_>) -> Self {
        let has_stripped_fields = union_.has_stripped_entries();
        let clean::Union { generics, fields } = union_;
        Union {
            generics: generics.into_json(renderer),
            has_stripped_fields,
            fields: renderer.ids(fields),
            impls: Vec::new(), // Added in JsonRenderer::item
        }
    }
}

impl FromClean<rustc_hir::FnHeader> for FunctionHeader {
    fn from_clean(header: &rustc_hir::FnHeader, renderer: &JsonRenderer<'_>) -> Self {
        let is_unsafe = match header.safety {
            HeaderSafety::SafeTargetFeatures => {
                // The type system's internal implementation details consider
                // safe functions with the `#[target_feature]` attribute to be analogous
                // to unsafe functions: `header.is_unsafe()` returns `true` for them.
                // For rustdoc, this isn't the right decision, so we explicitly return `false`.
                // Context: https://github.com/rust-lang/rust/issues/142655
                false
            }
            HeaderSafety::Normal(Safety::Safe) => false,
            HeaderSafety::Normal(Safety::Unsafe) => true,
        };
        FunctionHeader {
            is_async: header.is_async(),
            is_const: header.is_const(),
            is_unsafe,
            abi: header.abi.into_json(renderer),
        }
    }
}

impl FromClean<ExternAbi> for Abi {
    fn from_clean(a: &ExternAbi, _renderer: &JsonRenderer<'_>) -> Self {
        match *a {
            ExternAbi::Rust => Abi::Rust,
            ExternAbi::C { unwind } => Abi::C { unwind },
            ExternAbi::Cdecl { unwind } => Abi::Cdecl { unwind },
            ExternAbi::Stdcall { unwind } => Abi::Stdcall { unwind },
            ExternAbi::Fastcall { unwind } => Abi::Fastcall { unwind },
            ExternAbi::Aapcs { unwind } => Abi::Aapcs { unwind },
            ExternAbi::Win64 { unwind } => Abi::Win64 { unwind },
            ExternAbi::SysV64 { unwind } => Abi::SysV64 { unwind },
            ExternAbi::System { unwind } => Abi::System { unwind },
            _ => Abi::Other(a.to_string()),
        }
    }
}

impl FromClean<clean::Lifetime> for String {
    fn from_clean(l: &clean::Lifetime, _renderer: &JsonRenderer<'_>) -> String {
        l.0.to_string()
    }
}

impl FromClean<clean::Generics> for Generics {
    fn from_clean(generics: &clean::Generics, renderer: &JsonRenderer<'_>) -> Self {
        Generics {
            params: generics.params.into_json(renderer),
            where_predicates: generics.where_predicates.into_json(renderer),
        }
    }
}

impl FromClean<clean::GenericParamDef> for GenericParamDef {
    fn from_clean(generic_param: &clean::GenericParamDef, renderer: &JsonRenderer<'_>) -> Self {
        GenericParamDef {
            name: generic_param.name.to_string(),
            kind: generic_param.kind.into_json(renderer),
        }
    }
}

impl FromClean<clean::GenericParamDefKind> for GenericParamDefKind {
    fn from_clean(kind: &clean::GenericParamDefKind, renderer: &JsonRenderer<'_>) -> Self {
        use clean::GenericParamDefKind::*;
        match kind {
            Lifetime { outlives } => {
                GenericParamDefKind::Lifetime { outlives: outlives.into_json(renderer) }
            }
            Type { bounds, default, synthetic } => GenericParamDefKind::Type {
                bounds: bounds.into_json(renderer),
                implied_bounds: Vec::new(),
                default: default.into_json(renderer),
                is_synthetic: *synthetic,
            },
            Const { ty, default } => GenericParamDefKind::Const {
                type_: ty.into_json(renderer),
                default: default.as_ref().map(|x| x.as_ref().clone()),
            },
        }
    }
}

impl FromClean<clean::WherePredicate> for WherePredicate {
    fn from_clean(predicate: &clean::WherePredicate, renderer: &JsonRenderer<'_>) -> Self {
        use clean::WherePredicate::*;
        match predicate {
            BoundPredicate { ty, bounds, bound_params } => WherePredicate::BoundPredicate {
                type_: ty.into_json(renderer),
                bounds: bounds.into_json(renderer),
                generic_params: bound_params.into_json(renderer),
            },
            RegionPredicate { lifetime, bounds } => WherePredicate::LifetimePredicate {
                lifetime: lifetime.into_json(renderer),
                outlives: bounds
                    .iter()
                    .map(|bound| match bound {
                        clean::GenericBound::Outlives(lt) => lt.into_json(renderer),
                        _ => bug!("found non-outlives-bound on lifetime predicate"),
                    })
                    .collect(),
            },
            EqPredicate { lhs, rhs } => WherePredicate::EqPredicate {
                // The LHS currently has type `Type` but it should be a `QualifiedPath` since it may
                // refer to an associated const. However, `EqPredicate` shouldn't exist in the first
                // place: <https://github.com/rust-lang/rust/141368>.
                lhs: lhs.into_json(renderer),
                rhs: rhs.into_json(renderer),
            },
        }
    }
}

impl FromClean<clean::GenericBound> for GenericBound {
    fn from_clean(bound: &clean::GenericBound, renderer: &JsonRenderer<'_>) -> Self {
        use clean::GenericBound::*;
        match bound {
            TraitBound(clean::PolyTrait { trait_, generic_params }, modifier) => {
                GenericBound::TraitBound {
                    trait_: trait_.into_json(renderer),
                    generic_params: generic_params.into_json(renderer),
                    modifier: modifier.into_json(renderer),
                }
            }
            Outlives(lifetime) => GenericBound::Outlives(lifetime.into_json(renderer)),
            Use(args) => GenericBound::Use(
                args.iter()
                    .map(|arg| match arg {
                        clean::PreciseCapturingArg::Lifetime(lt) => {
                            PreciseCapturingArg::Lifetime(lt.into_json(renderer))
                        }
                        clean::PreciseCapturingArg::Param(param) => {
                            PreciseCapturingArg::Param(param.to_string())
                        }
                    })
                    .collect(),
            ),
        }
    }
}

impl FromClean<rustc_hir::TraitBoundModifiers> for TraitBoundModifier {
    fn from_clean(
        modifiers: &rustc_hir::TraitBoundModifiers,
        _renderer: &JsonRenderer<'_>,
    ) -> Self {
        use rustc_hir as hir;
        let hir::TraitBoundModifiers { constness, polarity } = modifiers;
        match (constness, polarity) {
            (hir::BoundConstness::Never, hir::BoundPolarity::Positive) => TraitBoundModifier::None,
            (hir::BoundConstness::Never, hir::BoundPolarity::Maybe(_)) => TraitBoundModifier::Maybe,
            (hir::BoundConstness::Maybe(_), hir::BoundPolarity::Positive) => {
                TraitBoundModifier::MaybeConst
            }
            // FIXME: Fill out the rest of this matrix.
            _ => TraitBoundModifier::None,
        }
    }
}

impl FromClean<clean::Type> for Type {
    fn from_clean(ty: &clean::Type, renderer: &JsonRenderer<'_>) -> Self {
        use clean::Type::{
            Array, BareFunction, BorrowedRef, Generic, ImplTrait, Infer, Primitive, QPath,
            RawPointer, SelfTy, Slice, Tuple, UnsafeBinder,
        };

        match ty {
            clean::Type::Path { path } => Type::ResolvedPath(path.into_json(renderer)),
            clean::Type::DynTrait(bounds, lt) => Type::DynTrait(DynTrait {
                lifetime: lt.into_json(renderer),
                traits: bounds.into_json(renderer),
            }),
            Generic(s) => Type::Generic(s.to_string()),
            // FIXME: add dedicated variant to json Type?
            SelfTy => Type::Generic("Self".to_owned()),
            Primitive(p) => Type::Primitive(p.as_sym().to_string()),
            BareFunction(f) => Type::FunctionPointer(Box::new(f.into_json(renderer))),
            Tuple(t) => Type::Tuple(t.into_json(renderer)),
            Slice(t) => Type::Slice(Box::new(t.into_json(renderer))),
            Array(t, s) => {
                Type::Array { type_: Box::new(t.into_json(renderer)), len: s.to_string() }
            }
            clean::Type::Pat(t, p) => Type::Pat {
                type_: Box::new(t.into_json(renderer)),
                __pat_unstable_do_not_use: p.to_string(),
            },
            ImplTrait { bounds, origin } => {
                let bounds_json: Vec<GenericBound> = bounds.into_json(renderer);
                let implied_bounds = renderer.implied_bounds_for_impl_trait(origin, &bounds_json);
                Type::ImplTrait { bounds: bounds_json, implied_bounds }
            }
            Infer => Type::Infer,
            RawPointer(mutability, type_) => Type::RawPointer {
                is_mutable: *mutability == ast::Mutability::Mut,
                type_: Box::new(type_.into_json(renderer)),
            },
            BorrowedRef { lifetime, mutability, type_ } => Type::BorrowedRef {
                lifetime: lifetime.into_json(renderer),
                is_mutable: *mutability == ast::Mutability::Mut,
                type_: Box::new(type_.into_json(renderer)),
            },
            QPath(qpath) => qpath.into_json(renderer),
            // FIXME(unsafe_binder): Implement rustdoc-json.
            UnsafeBinder(_) => todo!(),
        }
    }
}

impl FromClean<clean::Path> for Path {
    fn from_clean(path: &clean::Path, renderer: &JsonRenderer<'_>) -> Self {
        Path {
            path: path.whole_name(),
            id: renderer.id_from_item_default(path.def_id().into()),
            args: {
                if let Some((final_seg, rest_segs)) = path.segments.split_last() {
                    // In general, `clean::Path` can hold things like
                    // `std::vec::Vec::<u32>::new`, where generic args appear
                    // in a middle segment. But for the places where `Path` is
                    // used by rustdoc-json-types, generic args can only be
                    // used in the final segment, e.g. `std::vec::Vec<u32>`. So
                    // check that the non-final segments have no generic args.
                    assert!(rest_segs.iter().all(|seg| seg.args.is_empty()));
                    final_seg.args.into_json(renderer)
                } else {
                    None // no generics on any segments because there are no segments
                }
            },
        }
    }
}

impl FromClean<clean::QPathData> for Type {
    fn from_clean(qpath: &clean::QPathData, renderer: &JsonRenderer<'_>) -> Self {
        let clean::QPathData { assoc, self_type, should_fully_qualify: _, trait_ } = qpath;

        Self::QualifiedPath {
            name: assoc.name.to_string(),
            args: assoc.args.into_json(renderer),
            self_type: Box::new(self_type.into_json(renderer)),
            trait_: trait_.into_json(renderer),
        }
    }
}

impl FromClean<clean::Term> for Term {
    fn from_clean(term: &clean::Term, renderer: &JsonRenderer<'_>) -> Self {
        match term {
            clean::Term::Type(ty) => Term::Type(ty.into_json(renderer)),
            clean::Term::Constant(c) => Term::Constant(c.into_json(renderer)),
        }
    }
}

impl FromClean<clean::BareFunctionDecl> for FunctionPointer {
    fn from_clean(bare_decl: &clean::BareFunctionDecl, renderer: &JsonRenderer<'_>) -> Self {
        let clean::BareFunctionDecl { safety, generic_params, decl, abi } = bare_decl;
        FunctionPointer {
            header: FunctionHeader {
                is_unsafe: safety.is_unsafe(),
                is_const: false,
                is_async: false,
                abi: abi.into_json(renderer),
            },
            generic_params: generic_params.into_json(renderer),
            sig: decl.into_json(renderer),
        }
    }
}

impl FromClean<clean::FnDecl> for FunctionSignature {
    fn from_clean(decl: &clean::FnDecl, renderer: &JsonRenderer<'_>) -> Self {
        let clean::FnDecl { inputs, output, c_variadic } = decl;
        FunctionSignature {
            inputs: inputs
                .iter()
                .map(|param| {
                    // `_` is the most sensible name for missing param names.
                    let name = param.name.unwrap_or(kw::Underscore).to_string();
                    let type_ = param.type_.into_json(renderer);
                    (name, type_)
                })
                .collect(),
            output: if output.is_unit() { None } else { Some(output.into_json(renderer)) },
            is_c_variadic: *c_variadic,
        }
    }
}

fn from_clean_impl(impl_: &clean::Impl, owner_def_id: DefId, renderer: &JsonRenderer<'_>) -> Impl {
    let provided_trait_methods = impl_.provided_trait_methods(renderer.tcx);
    let clean::Impl { safety, generics, trait_, for_, items, polarity, kind } = impl_;
    // FIXME: use something like ImplKind in JSON?
    let (is_synthetic, blanket_impl) = match kind {
        clean::ImplKind::Normal | clean::ImplKind::FakeVariadic => (false, None),
        clean::ImplKind::Auto => (true, None),
        clean::ImplKind::Blanket(ty) => (false, Some(ty)),
    };
    let is_negative = matches!(polarity, ty::ImplPolarity::Negative);
    Impl {
        is_unsafe: safety.is_unsafe(),
        generics: renderer.generics_into_json(generics, owner_def_id),
        provided_trait_methods: provided_trait_methods.into_iter().map(|x| x.to_string()).collect(),
        trait_: trait_.into_json(renderer),
        for_: for_.into_json(renderer),
        items: renderer.ids(items),
        is_negative,
        is_synthetic,
        blanket_impl: blanket_impl.map(|x| x.into_json(renderer)),
    }
}

impl FromClean<clean::Trait> for Trait {
    fn from_clean(trait_: &clean::Trait, renderer: &JsonRenderer<'_>) -> Self {
        let tcx = renderer.tcx;
        let is_auto = trait_.is_auto(tcx);
        let is_unsafe = trait_.safety(tcx).is_unsafe();
        let is_dyn_compatible = trait_.is_dyn_compatible(tcx);
        let clean::Trait { items, generics, bounds, def_id, .. } = trait_;
        Trait {
            is_auto,
            is_unsafe,
            is_dyn_compatible,
            items: renderer.ids(items),
            generics: renderer.generics_into_json(generics, *def_id),
            bounds: bounds.into_json(renderer),
            implementations: Vec::new(), // Added in JsonRenderer::item
        }
    }
}

impl FromClean<clean::PolyTrait> for PolyTrait {
    fn from_clean(
        clean::PolyTrait { trait_, generic_params }: &clean::PolyTrait,
        renderer: &JsonRenderer<'_>,
    ) -> Self {
        PolyTrait {
            trait_: trait_.into_json(renderer),
            generic_params: generic_params.into_json(renderer),
        }
    }
}

impl FromClean<clean::Impl> for Impl {
    fn from_clean(impl_: &clean::Impl, renderer: &JsonRenderer<'_>) -> Self {
        let provided_trait_methods = impl_.provided_trait_methods(renderer.tcx);
        let clean::Impl { safety, generics, trait_, for_, items, polarity, kind, is_deprecated: _ } =
            impl_;
        // FIXME: use something like ImplKind in JSON?
        let (is_synthetic, blanket_impl) = match kind {
            clean::ImplKind::Normal | clean::ImplKind::FakeVariadic => (false, None),
            clean::ImplKind::Auto => (true, None),
            clean::ImplKind::Blanket(ty) => (false, Some(ty)),
        };
        let is_negative = match polarity {
            ty::ImplPolarity::Positive | ty::ImplPolarity::Reservation => false,
            ty::ImplPolarity::Negative => true,
        };
        Impl {
            is_unsafe: safety.is_unsafe(),
            generics: generics.into_json(renderer),
            provided_trait_methods: provided_trait_methods
                .into_iter()
                .map(|x| x.to_string())
                .collect(),
            trait_: trait_.into_json(renderer),
            for_: for_.into_json(renderer),
            items: renderer.ids(items),
            is_negative,
            is_synthetic,
            blanket_impl: blanket_impl.map(|x| x.into_json(renderer)),
        }
    }
}

pub(crate) fn from_clean_function(
    clean::Function { decl, generics }: &clean::Function,
    owner_def_id: DefId,
    has_body: bool,
    header: rustc_hir::FnHeader,
    renderer: &JsonRenderer<'_>,
) -> Function {
    Function {
        sig: decl.into_json(renderer),
        generics: renderer.generics_into_json(generics, owner_def_id),
        header: header.into_json(renderer),
        has_body,
    }
}

impl FromClean<clean::Enum> for Enum {
    fn from_clean(enum_: &clean::Enum, renderer: &JsonRenderer<'_>) -> Self {
        let has_stripped_variants = enum_.has_stripped_entries();
        let clean::Enum { variants, generics } = enum_;
        Enum {
            generics: generics.into_json(renderer),
            has_stripped_variants,
            variants: renderer.ids(&variants.as_slice().raw),
            impls: Vec::new(), // Added in JsonRenderer::item
        }
    }
}

impl FromClean<clean::Variant> for Variant {
    fn from_clean(variant: &clean::Variant, renderer: &JsonRenderer<'_>) -> Self {
        use clean::VariantKind::*;

        let discriminant = variant.discriminant.into_json(renderer);

        let kind = match &variant.kind {
            CLike => VariantKind::Plain,
            Tuple(fields) => VariantKind::Tuple(renderer.ids_keeping_stripped(fields)),
            Struct(s) => VariantKind::Struct {
                has_stripped_fields: s.has_stripped_entries(),
                fields: renderer.ids(&s.fields),
            },
        };

        Variant { kind, discriminant }
    }
}

impl FromClean<clean::Discriminant> for Discriminant {
    fn from_clean(disr: &clean::Discriminant, renderer: &JsonRenderer<'_>) -> Self {
        let tcx = renderer.tcx;
        Discriminant {
            // expr is only none if going through the inlining path, which gets
            // `rustc_middle` types, not `rustc_hir`, but because JSON never inlines
            // the expr is always some.
            expr: disr.expr(tcx).unwrap(),
            value: disr.value(tcx, false),
        }
    }
}

impl FromClean<clean::Import> for Use {
    fn from_clean(import: &clean::Import, renderer: &JsonRenderer<'_>) -> Self {
        use clean::ImportKind::*;
        let (name, is_glob) = match import.kind {
            Simple(s) => (s.to_string(), false),
            Glob => (import.source.path.last_opt().unwrap_or(sym::asterisk).to_string(), true),
        };
        Use {
            source: import.source.path.whole_name(),
            name,
            id: import.source.did.map(ItemId::from).map(|i| renderer.id_from_item_default(i)),
            is_glob,
        }
    }
}

impl FromClean<clean::ProcMacro> for ProcMacro {
    fn from_clean(mac: &clean::ProcMacro, renderer: &JsonRenderer<'_>) -> Self {
        ProcMacro {
            kind: mac.kind.into_json(renderer),
            helpers: mac.helpers.iter().map(|x| x.to_string()).collect(),
        }
    }
}

impl FromClean<rustc_span::hygiene::MacroKind> for MacroKind {
    fn from_clean(kind: &rustc_span::hygiene::MacroKind, _renderer: &JsonRenderer<'_>) -> Self {
        use rustc_span::hygiene::MacroKind::*;
        match kind {
            Bang => MacroKind::Bang,
            Attr => MacroKind::Attr,
            Derive => MacroKind::Derive,
        }
    }
}

impl FromClean<clean::TypeAlias> for TypeAlias {
    fn from_clean(type_alias: &clean::TypeAlias, renderer: &JsonRenderer<'_>) -> Self {
        let clean::TypeAlias { type_, generics, item_type: _, inner_type: _ } = type_alias;
        TypeAlias { type_: type_.into_json(renderer), generics: generics.into_json(renderer) }
    }
}

fn from_clean_static(
    stat: &clean::Static,
    safety: rustc_hir::Safety,
    renderer: &JsonRenderer<'_>,
) -> Static {
    let tcx = renderer.tcx;
    Static {
        type_: stat.type_.as_ref().into_json(renderer),
        is_mutable: stat.mutability == ast::Mutability::Mut,
        is_unsafe: safety.is_unsafe(),
        expr: stat
            .expr
            .map(|e| rendered_const(tcx, tcx.hir_body(e), tcx.hir_body_owner_def_id(e)))
            .unwrap_or_default(),
    }
}

impl FromClean<clean::TraitAlias> for TraitAlias {
    fn from_clean(alias: &clean::TraitAlias, renderer: &JsonRenderer<'_>) -> Self {
        TraitAlias {
            generics: alias.generics.into_json(renderer),
            params: alias.bounds.into_json(renderer),
        }
    }
}

impl FromClean<ItemType> for ItemKind {
    fn from_clean(kind: &ItemType, _renderer: &JsonRenderer<'_>) -> Self {
        use ItemType::*;
        match kind {
            Module => ItemKind::Module,
            ExternCrate => ItemKind::ExternCrate,
            Import => ItemKind::Use,
            Struct => ItemKind::Struct,
            Union => ItemKind::Union,
            Enum => ItemKind::Enum,
            Function | TyMethod | Method => ItemKind::Function,
            TypeAlias => ItemKind::TypeAlias,
            Static => ItemKind::Static,
            Constant => ItemKind::Constant,
            Trait => ItemKind::Trait,
            Impl => ItemKind::Impl,
            StructField => ItemKind::StructField,
            Variant => ItemKind::Variant,
            Macro => ItemKind::Macro,
            Primitive => ItemKind::Primitive,
            AssocConst => ItemKind::AssocConst,
            AssocType => ItemKind::AssocType,
            ForeignType => ItemKind::ExternType,
            Keyword => ItemKind::Keyword,
            Attribute => ItemKind::Attribute,
            TraitAlias => ItemKind::TraitAlias,
            ProcAttribute => ItemKind::ProcAttribute,
            ProcDerive => ItemKind::ProcDerive,
        }
    }
}

/// Maybe convert a attribute from hir to json.
///
/// Returns `None` if the attribute shouldn't be in the output.
fn maybe_from_hir_attr(attr: &hir::Attribute, item_id: ItemId, tcx: TyCtxt<'_>) -> Vec<Attribute> {
    use attrs::AttributeKind as AK;

    let kind = match attr {
        hir::Attribute::Parsed(kind) => kind,

        hir::Attribute::Unparsed(_) => {
            // FIXME: We should handle `#[doc(hidden)]`.
            return vec![other_attr(tcx, attr)];
        }
    };

    vec![match kind {
        AK::Deprecation { .. } => return Vec::new(), // Handled separately into Item::deprecation.
        AK::DocComment { .. } => unreachable!("doc comments stripped out earlier"),

        AK::MacroExport { .. } => Attribute::MacroExport,
        AK::MustUse { reason, span: _ } => {
            Attribute::MustUse { reason: reason.map(|s| s.to_string()) }
        }
        AK::Repr { .. } => repr_attr(
            tcx,
            item_id.as_def_id().expect("all items that could have #[repr] have a DefId"),
        ),
        AK::ExportName { name, span: _ } => Attribute::ExportName(name.to_string()),
        AK::LinkSection { name, span: _ } => Attribute::LinkSection(name.to_string()),
        AK::TargetFeature { features, .. } => Attribute::TargetFeature {
            enable: features.iter().map(|(feat, _span)| feat.to_string()).collect(),
        },

        AK::NoMangle(_) => Attribute::NoMangle,
        AK::NonExhaustive(_) => Attribute::NonExhaustive,
        AK::AutomaticallyDerived(_) => Attribute::AutomaticallyDerived,
        AK::Doc(d) => {
            fn toggle_attr(ret: &mut Vec<Attribute>, name: &str, v: &Option<rustc_span::Span>) {
                if v.is_some() {
                    ret.push(Attribute::Other(format!("#[doc({name})]")));
                }
            }

            fn name_value_attr(
                ret: &mut Vec<Attribute>,
                name: &str,
                v: &Option<(Symbol, rustc_span::Span)>,
            ) {
                if let Some((v, _)) = v {
                    // We use `as_str` and debug display to have characters escaped and `"`
                    // characters surrounding the string.
                    ret.push(Attribute::Other(format!("#[doc({name} = {:?})]", v.as_str())));
                }
            }

            let DocAttribute {
                aliases,
                hidden,
                inline,
                cfg,
                auto_cfg,
                auto_cfg_change,
                fake_variadic,
                keyword,
                attribute,
                masked,
                notable_trait,
                search_unbox,
                html_favicon_url,
                html_logo_url,
                html_playground_url,
                html_root_url,
                html_no_source,
                issue_tracker_base_url,
                rust_logo,
                test_attrs,
                no_crate_inject,
            } = &**d;

            let mut ret = Vec::new();

            for (alias, _) in aliases {
                // We use `as_str` and debug display to have characters escaped and `"` characters
                // surrounding the string.
                ret.push(Attribute::Other(format!("#[doc(alias = {:?})]", alias.as_str())));
            }
            toggle_attr(&mut ret, "hidden", hidden);
            if let Some(inline) = inline.first() {
                ret.push(Attribute::Other(format!(
                    "#[doc({})]",
                    match inline.0 {
                        DocInline::Inline => "inline",
                        DocInline::NoInline => "no_inline",
                    }
                )));
            }
            for sub_cfg in cfg {
                ret.push(Attribute::Other(format!("#[doc(cfg({sub_cfg}))]")));
            }
            for (auto_cfg, _) in auto_cfg {
                let kind = match auto_cfg.kind {
                    HideOrShow::Hide => "hide",
                    HideOrShow::Show => "show",
                };
                let mut out = format!("#[doc(auto_cfg({kind}(");
                for (pos, value) in auto_cfg.values.iter().enumerate() {
                    if pos > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(value.name.as_str());
                    if let Some((value, _)) = value.value {
                        // We use `as_str` and debug display to have characters escaped and `"`
                        // characters surrounding the string.
                        out.push_str(&format!(" = {:?}", value.as_str()));
                    }
                }
                out.push_str(")))]");
                ret.push(Attribute::Other(out));
            }
            for (change, _) in auto_cfg_change {
                ret.push(Attribute::Other(format!("#[doc(auto_cfg = {change})]")));
            }
            toggle_attr(&mut ret, "fake_variadic", fake_variadic);
            name_value_attr(&mut ret, "keyword", keyword);
            name_value_attr(&mut ret, "attribute", attribute);
            toggle_attr(&mut ret, "masked", masked);
            toggle_attr(&mut ret, "notable_trait", notable_trait);
            toggle_attr(&mut ret, "search_unbox", search_unbox);
            name_value_attr(&mut ret, "html_favicon_url", html_favicon_url);
            name_value_attr(&mut ret, "html_logo_url", html_logo_url);
            name_value_attr(&mut ret, "html_playground_url", html_playground_url);
            name_value_attr(&mut ret, "html_root_url", html_root_url);
            toggle_attr(&mut ret, "html_no_source", html_no_source);
            name_value_attr(&mut ret, "issue_tracker_base_url", issue_tracker_base_url);
            toggle_attr(&mut ret, "rust_logo", rust_logo);
            let source_map = tcx.sess.source_map();
            for attr_span in test_attrs {
                // FIXME: This is ugly, remove when `test_attrs` has been ported to new attribute API.
                if let Ok(snippet) = source_map.span_to_snippet(*attr_span) {
                    ret.push(Attribute::Other(format!("#[doc(test(attr({snippet})))")));
                }
            }
            toggle_attr(&mut ret, "no_crate_inject", no_crate_inject);
            return ret;
        }

        _ => other_attr(tcx, attr),
    }]
}

fn other_attr(tcx: TyCtxt<'_>, attr: &hir::Attribute) -> Attribute {
    let mut s = rustc_hir_pretty::attribute_to_string(&tcx, attr);
    assert_eq!(s.pop(), Some('\n'));
    Attribute::Other(s)
}

fn repr_attr(tcx: TyCtxt<'_>, def_id: DefId) -> Attribute {
    let repr = tcx.adt_def(def_id).repr();

    let kind = if repr.c() {
        ReprKind::C
    } else if repr.transparent() {
        ReprKind::Transparent
    } else if repr.simd() {
        ReprKind::Simd
    } else {
        ReprKind::Rust
    };

    let align = repr.align.map(|a| a.bytes());
    let packed = repr.pack.map(|p| p.bytes());
    let int = repr.int.map(format_integer_type);

    Attribute::Repr(AttributeRepr { kind, align, packed, int })
}

fn format_integer_type(it: rustc_abi::IntegerType) -> String {
    use rustc_abi::Integer::*;
    use rustc_abi::IntegerType::*;
    match it {
        Pointer(true) => "isize",
        Pointer(false) => "usize",
        Fixed(I8, true) => "i8",
        Fixed(I8, false) => "u8",
        Fixed(I16, true) => "i16",
        Fixed(I16, false) => "u16",
        Fixed(I32, true) => "i32",
        Fixed(I32, false) => "u32",
        Fixed(I64, true) => "i64",
        Fixed(I64, false) => "u64",
        Fixed(I128, true) => "i128",
        Fixed(I128, false) => "u128",
    }
    .to_owned()
}

pub(super) fn target(sess: &rustc_session::Session) -> Target {
    // Build a set of which features are enabled on this target
    let globally_enabled_features: FxHashSet<&str> =
        sess.unstable_target_features.iter().map(|name| name.as_str()).collect();

    // Build a map of target feature stability by feature name
    use rustc_target::target_features::Stability;
    let feature_stability: FxHashMap<&str, Stability> = sess
        .target
        .rust_target_features()
        .iter()
        .copied()
        .map(|(name, stability, _)| (name, stability))
        .collect();

    Target {
        triple: sess.opts.target_triple.tuple().into(),
        target_features: sess
            .target
            .rust_target_features()
            .iter()
            .copied()
            .filter(|(_, stability, _)| {
                // Describe only target features which the user can toggle
                stability.toggle_allowed().is_ok()
            })
            .map(|(name, stability, implied_features)| {
                TargetFeature {
                    name: name.into(),
                    unstable_feature_gate: match stability {
                        Stability::Unstable(feature_gate) => Some(feature_gate.as_str().into()),
                        _ => None,
                    },
                    implies_features: implied_features
                        .iter()
                        .copied()
                        .filter(|name| {
                            // Imply only target features which the user can toggle
                            feature_stability
                                .get(name)
                                .map(|stability| stability.toggle_allowed().is_ok())
                                .unwrap_or(false)
                        })
                        .map(String::from)
                        .collect(),
                    globally_enabled: globally_enabled_features.contains(name),
                }
            })
            .collect(),
    }
}
