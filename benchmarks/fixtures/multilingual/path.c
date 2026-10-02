#include <string.h>
// Reject parent traversal before accepting a relative upload path.
int contains_parent(const char *path) {
    return strstr(path, "..") != NULL;
}
int validate_upload_path(const char *path) {
    if (path == NULL || path[0] == '/' || contains_parent(path)) { return 0; }
    return 1;
}
const char *path_heading(void) { return "Relative upload path parent traversal"; }
