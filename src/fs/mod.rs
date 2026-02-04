use std::{collections::BTreeMap, sync::LazyLock};

pub static FILES: LazyLock<BTreeMap<&str, &[u8]>> = LazyLock::new(|| BTreeMap::from([
    ("/boards.bml", &include_bytes!("boards.bml")[..]),
    ("/BSMemory.bml", include_bytes!("BSMemory.bml")),
    ("/SufamiTurbo.bml", include_bytes!("SufamiTurbo.bml")),
    ("/SuperFamicom.bml", include_bytes!("SuperFamicom.bml")),
]));