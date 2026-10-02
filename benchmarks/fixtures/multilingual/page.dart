class PageCursor {
  int clampPageSize(int requested) {
    return requested.clamp(1, 100);
  }
  List<int> nextPage(List<int> rows, int cursor, int requested) {
    final size = clampPageSize(requested);
    return rows.where((row) => row > cursor).take(size).toList();
  }
  String pageHeading() { return 'Next page cursor requested size rows'; }
}
