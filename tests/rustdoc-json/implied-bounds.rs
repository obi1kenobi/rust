//@ has "$.index[?(@.name=='unsized_ok')]"
//@ has "$.index[?(@.name=='sized_through_trait')]"
//@ has "$.index[?(@.name=='static_via_trait')]"
//@ has "$.index[?(@.name=='carries_inner')]"

pub trait Private: Sized {}

pub fn unsized_ok<T: ?Sized>(_value: &T) {}

pub fn sized_through_trait<T: Private + ?Sized>(_value: &T) {}

pub trait StaticPrivate: 'static {}
impl<T: 'static> StaticPrivate for T {}

pub fn static_via_trait<T: StaticPrivate>(_value: T) {}

pub fn explicit_sized<T: Sized>(_value: T) {}

pub struct Inner<'a, T>(&'a T);

pub fn carries_inner<'a, T>(lt: &'a i64, ty: T) -> Inner<'a, T> {
    let _ = lt;
    let _ = ty;
    todo!()
}

//@ count "$.index[?(@.name=='unsized_ok')].inner.function.generics.params[0].kind.type.implied_bounds[*]" 0
//@ has   "$.index[?(@.name=='sized_through_trait')].inner.function.generics.params[0].kind.type.implied_bounds[?(@.trait_bound.trait.path=='Sized')]"
//@ !has  "$.index[?(@.name=='sized_through_trait')].inner.function.generics.params[0].kind.type.implied_bounds[?(@.trait_bound.trait.path=='Private')]"
//@ has   "$.index[?(@.name=='static_via_trait')].inner.function.generics.params[0].kind.type.implied_bounds[?(@.trait_bound.trait.path=='Sized')]"
//@ has   "$.index[?(@.name=='static_via_trait')].inner.function.generics.params[0].kind.type.implied_bounds[?(@.outlives==\"'static\")]"
//@ !has  "$.index[?(@.name=='static_via_trait')].inner.function.generics.params[0].kind.type.implied_bounds[?(@.trait_bound.trait.path=='StaticPrivate')]"
//@ count "$.index[?(@.name=='explicit_sized')].inner.function.generics.params[0].kind.type.implied_bounds[*]" 0
//@ has   "$.index[?(@.name=='carries_inner')].inner.function.generics.params[1].kind.type.implied_bounds[?(@.trait_bound.trait.path=='Sized')]"
//@ has   "$.index[?(@.name=='Inner')].inner.struct.generics.params[1].kind.type.implied_bounds[?(@.trait_bound.trait.path=='Sized')]"
//@ has   "$.index[?(@.name=='Inner')].inner.struct.generics.params[1].kind.type.implied_bounds[?(@.outlives==\"'a\")]"
