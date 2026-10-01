from pathlib import Path

def resolve_workspace(root, relative):
    """Resolve a relative filename inside the workspace."""
    return (Path(root) / relative).resolve()

def is_within_workspace(root, relative):
    """Reject paths that escape the workspace directory."""
    return resolve_workspace(root, relative).is_relative_to(Path(root).resolve())

def read_workspace_file(root, relative):
    """Read a workspace file only after checking path traversal."""
    if not is_within_workspace(root, relative):
        raise ValueError("path traversal")
    return resolve_workspace(root, relative).read_text()

def workspace_breadcrumb():
    return "Workspace files"
