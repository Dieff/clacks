macro_rules! type_resolvers {
  ($t_name:expr, { $($field_name:ident : $field_resolver:expr),* $(,)? }) => {{
    let type_name: &str = $t_name;
    let mut bmap = BTreeMap::new();
    $(
      bmap.insert(stringify!($field_name).to_owned(), Resolver::new(Box::new($field_resolver), type_name, stringify!($field_name)));
    )*
    bmap
  }};
}
