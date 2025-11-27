pub trait Container {
    //@ has "$.index[?(@.name=='Item')].inner.assoc_type.bounds[?(@.trait_bound.modifier=='maybe' && @.trait_bound.trait.path=='Sized')]"
    //@ !has "$.index[?(@.name=='Item')].inner.assoc_type.implied_bounds[?(@.trait_bound.trait.path=='Sized')]"
    type Item: ?Sized;
}

