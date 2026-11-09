use crate::{
    Result, Session, cmd,
    commands::{CommandSpec, always, bad, strp},
    folder_path,
};
use serde_json::{Value, json};

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("folder.create", "New folder", [], None, "{parent?, name?} creates an empty folder", always, create),
        cmd!("folder.rename", "Rename folder", [], None, "{folder, name} renames a folder and updates local graph references", always, rename),
        cmd!("folder.remove", "Remove folder", [], None, "{folder} removes a folder and its contents, with undo", always, remove),
        cmd!("explorer.move", "Move Explorer item", [], None, "{graph? index, folder?, parent?} moves a graph or folder within its package", always, move_item),
    ]
}

fn create(session: &mut Session, parameters: &Value) -> Result<Value> {
    let parent = strp(parameters, "parent").unwrap_or("");
    folder_path::require(&session.pkg, parent)?;
    let name = folder_path::name(strp(parameters, "name").unwrap_or("Untitled_Folder"))?;
    let path = folder_path::join(parent, &folder_path::unique(&session.pkg, parent, name));
    folder_path::validate(&path)?;
    session.edit("New folder", |session| {
        session.pkg.folders.push(path.clone());
        Ok(json!({"folder": path}))
    })
}

fn source<'a>(session: &Session, parameters: &'a Value) -> Result<&'a str> {
    let path = strp(parameters, "folder").filter(|path| !path.is_empty()).ok_or_else(|| bad("folder", "folder is required"))?;
    folder_path::require(&session.pkg, path)?;
    Ok(path)
}

fn rename(session: &mut Session, parameters: &Value) -> Result<Value> {
    let old = source(session, parameters)?;
    let name = folder_path::name(strp(parameters, "name").ok_or_else(|| bad("folder.rename", "name is required"))?)?;
    let parent = old.rsplit_once('/').map_or("", |(parent, _)| parent);
    relocate_folder(session, old, &folder_path::join(parent, name))
}

fn relocate_folder(session: &mut Session, old: &str, new: &str) -> Result<Value> {
    folder_path::validate(new)?;
    if old == new {
        return Ok(json!({"folder": new}));
    }
    if folder_path::contains(old, new)
        || session.pkg.folder_paths().contains(new)
        || session.pkg.graphs.iter().any(|graph| graph.sbs_path() == new)
        || session.pkg.resources.iter().any(|resource| resource.sbs_path() == new)
    {
        return Err(bad("folder", "the destination already exists or is inside the source folder"));
    }

    let replace = |path: &str| format!("{new}{}", path.strip_prefix(old).unwrap_or(""));
    let folders = session.pkg.folder_paths().into_iter().map(|path| if folder_path::contains(old, &path) { replace(&path) } else { path }).collect::<Vec<_>>();
    for folder in &folders {
        folder_path::validate(folder)?;
    }
    let paths = session
        .pkg
        .graphs
        .iter()
        .filter(|graph| folder_path::contains(old, &graph.folder))
        .map(|graph| (graph.sbs_path(), folder_path::join(&replace(&graph.folder), &graph.name)))
        .collect();
    let names = crate::graph_references::with_leaf_names(&session.pkg, &paths);
    session.edit("Move or rename folder", |session| {
        for graph in &mut session.pkg.graphs {
            if folder_path::contains(old, &graph.folder) {
                graph.folder = replace(&graph.folder);
            }
        }
        session.pkg.folders = folders;
        for resource in &mut session.pkg.resources {
            if folder_path::contains(old, &resource.folder) {
                resource.folder = replace(&resource.folder);
            }
        }
        crate::graph_references::remap(&mut session.pkg, &names);
        Ok(json!({"folder": new}))
    })
}

fn remove(session: &mut Session, parameters: &Value) -> Result<Value> {
    let folder = source(session, parameters)?;
    session.edit("Remove folder", |session| {
        session.pkg.folders = session.pkg.folder_paths().into_iter().filter(|path| !folder_path::contains(folder, path)).collect();
        let indices: Vec<_> =
            session.pkg.graphs.iter().enumerate().filter(|(_, graph)| folder_path::contains(folder, &graph.folder)).map(|(index, _)| index).collect();
        for index in indices.into_iter().rev() {
            session.pkg.graphs.remove(index);
            session.material_preview.remove_graph(index);
            if index < session.graph {
                session.graph -= 1;
            }
        }
        session.pkg.resources.retain(|resource| !folder_path::contains(folder, &resource.folder));
        session.graph = session.graph.min(session.pkg.graphs.len().saturating_sub(1));
        session.selection.clear();
        session.annotation_selection = None;
        session.view2d.node = None;
        session.clear_preview_results();
        Ok(Value::Null)
    })
}

fn move_item(session: &mut Session, parameters: &Value) -> Result<Value> {
    let parent = strp(parameters, "parent").unwrap_or("");
    folder_path::require(&session.pkg, parent)?;
    if let Some(index) = parameters.get("graph").and_then(Value::as_u64).and_then(|index| usize::try_from(index).ok()) {
        let graph = session.pkg.graph(index)?;
        let new = folder_path::join(parent, &graph.name);
        if graph.folder == parent {
            return Ok(json!({"graph": index}));
        }
        if session.pkg.graphs.iter().any(|graph| graph.sbs_path() == new)
            || session.pkg.folder_paths().contains(&new)
            || session.pkg.resources.iter().any(|resource| resource.sbs_path() == new)
        {
            return Err(bad("explorer.move", "the destination already contains an item with this name"));
        }

        let paths = [(graph.sbs_path(), new)].into();
        let names = crate::graph_references::with_leaf_names(&session.pkg, &paths);
        return session.edit("Move graph", |session| {
            session.pkg.folders = session.pkg.folder_paths().into_iter().collect();
            session.pkg.graph_mut(index)?.folder = parent.into();
            crate::graph_references::remap(&mut session.pkg, &names);
            Ok(json!({"graph": index}))
        });
    }

    let old = source(session, parameters)?;
    let name = old.rsplit('/').next().unwrap_or(old);
    relocate_folder(session, old, &folder_path::join(parent, name))
}
