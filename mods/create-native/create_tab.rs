// Compiled inside horizon_client::creative::Creative::reopen, so the
// module's private helpers (java_lower, split_id, SearchEntry) are in scope.
// The content mod's items are known: the create.hmod declares exactly these
// blocks (cogwheel_off has no item — it is the idle model the kinetics mod
// swaps in). The tab joins the top row after the vanilla tabs; the stacks
// also join the search tab, with name/index entries so the search finds them.
fn create_tab(creative: &mut Creative) {
    if creative.tabs.iter().any(|t| t.id == "create") {
        return;
    }
    const BLOCKS: [&str; 8] = [
        "cogwheel",
        "large_cogwheel",
        "shaft",
        "gearbox",
        "creative_motor",
        "water_wheel",
        "windmill",
        "millstone",
    ];
    let items: Vec<ItemStack> = BLOCKS
        .iter()
        .map(|path| ItemStack::new(&format!("create:{path}"), 1))
        .collect();
    let column = creative
        .tabs
        .iter()
        .filter(|t| t.top)
        .map(|t| t.column)
        .max()
        .unwrap_or(0)
        + 1;
    creative.tabs.push(Tab {
        id: "create".to_owned(),
        title: "Create (RustCraft)".to_owned(),
        icon: "create:cogwheel".to_owned(),
        items: items.clone(),
        top: true,
        column,
        search: false,
        inventory: false,
    });
    for stack in items {
        let (namespace, path) = split_id(&java_lower(&stack.item));
        creative.search_index.push(SearchEntry {
            namespace,
            path,
            ..Default::default()
        });
        creative.search_items.push(stack);
    }
}
