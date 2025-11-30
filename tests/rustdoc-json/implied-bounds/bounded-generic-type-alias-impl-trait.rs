#![feature(type_alias_impl_trait)]

use std::fmt::Debug;

pub trait NeedsSized: Sized {}
impl<T: Sized> NeedsSized for T {}

pub trait StaticOnly: 'static {}
impl<T: 'static> StaticOnly for T {}

//@ has "$.index[?(@.name=='SizedParam')].inner.type_alias.generics.params[1].kind.type.bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
//@ has "$.index[?(@.name=='SizedParam')].inner.type_alias.generics.params[1].kind.type.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
//@ !has "$.index[?(@.name=='SizedParam')].inner.type_alias.generics.params[1].kind.type.implied_bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
pub type SizedParam<'a, T: NeedsSized> = impl Debug + PartialEq<T> + 'a;

//@ has "$.index[?(@.name=='MaybeSizedParam')].inner.type_alias.generics.params[1].kind.type.bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='maybe')]"
//@ !has "$.index[?(@.name=='MaybeSizedParam')].inner.type_alias.generics.params[1].kind.type.implied_bounds[?(@.trait_bound.trait.path=='Sized')]"
//@ !has "$.index[?(@.name=='MaybeSizedParam')].inner.type_alias.generics.params[1].kind.type.implied_bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
pub type MaybeSizedParam<'a, T: ?Sized> = impl Debug + PartialEq<T> + 'a;

//@ has "$.index[?(@.name=='SizedMaybeUnsized')].inner.type_alias.generics.params[1].kind.type.bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
//@ has "$.index[?(@.name=='SizedMaybeUnsized')].inner.type_alias.generics.params[1].kind.type.bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='maybe')]"
//@ !has "$.index[?(@.name=='SizedMaybeUnsized')].inner.type_alias.generics.params[1].kind.type.implied_bounds[?(@.trait_bound.trait.path=='NeedsSized')]"
//@ has "$.index[?(@.name=='SizedMaybeUnsized')].inner.type_alias.generics.params[1].kind.type.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
pub type SizedMaybeUnsized<'a, T: NeedsSized + ?Sized> = impl Debug + PartialEq<T> + 'a;

//@ has "$.index[?(@.name=='StaticParam')].inner.type_alias.generics.params[1].kind.type.bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
//@ has "$.index[?(@.name=='StaticParam')].inner.type_alias.generics.params[1].kind.type.implied_bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='none')]"
//@ has "$.index[?(@.name=='StaticParam')].inner.type_alias.generics.params[1].kind.type.implied_bounds[?(@.outlives==\"'static\")]"
//@ !has "$.index[?(@.name=='StaticParam')].inner.type_alias.generics.params[1].kind.type.implied_bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
pub type StaticParam<'a, T: StaticOnly> = impl Debug + PartialEq<T> + 'a;

//@ has "$.index[?(@.name=='StaticMaybeUnsized')].inner.type_alias.generics.params[1].kind.type.bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
//@ has "$.index[?(@.name=='StaticMaybeUnsized')].inner.type_alias.generics.params[1].kind.type.bounds[?(@.trait_bound.trait.path=='Sized' && @.trait_bound.modifier=='maybe')]"
//@ has "$.index[?(@.name=='StaticMaybeUnsized')].inner.type_alias.generics.params[1].kind.type.implied_bounds[?(@.outlives==\"'static\")]"
//@ !has "$.index[?(@.name=='StaticMaybeUnsized')].inner.type_alias.generics.params[1].kind.type.implied_bounds[?(@.trait_bound.trait.path=='StaticOnly')]"
//@ !has "$.index[?(@.name=='StaticMaybeUnsized')].inner.type_alias.generics.params[1].kind.type.implied_bounds[?(@.trait_bound.trait.path=='Sized')]"
pub type StaticMaybeUnsized<'a, T: StaticOnly + ?Sized> = impl Debug + PartialEq<T> + 'a;
