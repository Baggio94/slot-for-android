"""Puts the branch's render_shelves test into another copy of
shelves_grid.rs (main's), so both draw the same pages."""
import sys
from pathlib import Path
start, stop = '    fn render_shelves() {', '\n    #[test]'
def region(text):
    a = text.index(start)
    return a, text.index(stop, a)
src = Path(sys.argv[1]).read_text()
dst_path = Path(sys.argv[2]); dst = dst_path.read_text()
a, b = region(src); c, d = region(dst)
dst_path.write_text(dst[:c] + src[a:b] + dst[d:])
