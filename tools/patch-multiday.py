import re
path = r"C:\Users\hemsh_sfya5gq\notes+calendar\web\suite\static\calendar.html"
c = open(path, encoding="utf-8").read()

# clamp multi-day timed events to the visible day/column in both Day and Week views
old = """    const s = new Date(e.start), en = new Date(e.end);
    const startMin = (s.getHours() * 60 + s.getMinutes());
    const durMin = Math.max(15, (en - s) / 60000);"""
new = """    const s0 = new Date(e.start), e0 = new Date(e.end);
    const s = s0 < startOfDay(cur) ? startOfDay(cur) : s0;
    const en = e0 > addDays(startOfDay(cur), 1) ? addDays(startOfDay(cur), 1) : e0;
    const startMin = (s.getHours() * 60 + s.getMinutes());
    const durMin = Math.max(15, (en - s) / 60000);"""
assert c.count(old) == 2, f"expected 2 occurrences, found {c.count(old)}"
c = c.replace(old, new)
open(path, "w", encoding="utf-8").write(c)
print("multi-day clamp applied in Day and Week views")
