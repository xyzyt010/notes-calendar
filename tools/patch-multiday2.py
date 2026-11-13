path = r"C:\Users\hemsh_sfya5gq\notes+calendar\web\suite\static\calendar.html"
c = open(path, encoding="utf-8").read()

# Day view block (variable: cur)
day_old = """  const timed = visibleEvents().filter(e => !e.all_day && eventsIntersect(e, cur));
  const hourHeight = 52;
  for (const e of timed) {
    const s = new Date(e.start), en = new Date(e.end);
    const startMin = (s.getHours() * 60 + s.getMinutes());
    const durMin = Math.max(15, (en - s) / 60000);"""
day_new = """  const timed = visibleEvents().filter(e => !e.all_day && eventsIntersect(e, cur));
  const hourHeight = 52;
  const dayStart = startOfDay(cur), dayEnd = addDays(dayStart, 1);
  for (const e of timed) {
    const s0 = new Date(e.start), e0 = new Date(e.end);
    const s = s0 < dayStart ? dayStart : s0;
    const en = e0 > dayEnd ? dayEnd : e0;
    const startMin = (s.getHours() * 60 + s.getMinutes());
    const durMin = Math.max(15, (en - s) / 60000);"""
assert day_old in c, "day view block not found"
c = c.replace(day_old, day_new, 1)

# Week view block (variable: days[i] / d0)
week_old = """    const timed = visibleEvents().filter(e => !e.all_day && eventsIntersect(e, d0));
    for (const e of timed) {
      const s = new Date(e.start), en = new Date(e.end);
      const startMin = (s.getHours() * 60 + s.getMinutes());
      const durMin = Math.max(15, (en - s) / 60000);"""
week_new = """    const timed = visibleEvents().filter(e => !e.all_day && eventsIntersect(e, d0));
    const dStart = startOfDay(d0), dEnd = addDays(dStart, 1);
    for (const e of timed) {
      const s0 = new Date(e.start), e0 = new Date(e.end);
      const s = s0 < dStart ? dStart : s0;
      const en = e0 > dEnd ? dEnd : e0;
      const startMin = (s.getHours() * 60 + s.getMinutes());
      const durMin = Math.max(15, (en - s) / 60000);"""
assert week_old in c, "week view block not found"
c = c.replace(week_old, week_new, 1)

open(path, "w", encoding="utf-8").write(c)
print("multi-day clamp applied")
