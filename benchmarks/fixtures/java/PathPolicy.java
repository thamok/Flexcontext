package example.paths;

/** Repository relative paths must remain inside the workspace. */
public interface PathPolicy {
    /** Reject absolute paths and parent traversal components. */
    default boolean allowsRelativePath(String path) {
        if (path == null || path.startsWith("/")) {
            return false;
        }
        for (String component : path.split("/")) {
            if (component.equals("..")) {
                return false;
            }
        }
        return true;
    }

    /** A directory prefix matches the directory itself and slash-separated descendants. */
    default boolean matchesDirectoryPrefix(String path, String prefix) {
        return path.equals(prefix) || path.startsWith(prefix + "/");
    }

    String pathHeading();
}
