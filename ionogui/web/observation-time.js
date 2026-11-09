export const utcDay = (now = Date.now()) => new Date(now).toISOString().slice(0, 10);
export const utcMinute = (now = Date.now()) => new Date(now).toISOString().slice(11, 16);
export function selectionUnix(day, time) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(day) || !/^([01]\d|2[0-3]):[0-5]\d$/.test(time)) return null;
  const value = Date.parse(`${day}T${time}:00Z`) / 1000;
  return Number.isFinite(value) && utcDay(value * 1000) === day ? value : null;
}
export function calendarDays(month) {
  const first = new Date(`${month}-01T00:00:00Z`);
  const offset = (first.getUTCDay() + 6) % 7;
  const count = new Date(Date.UTC(first.getUTCFullYear(), first.getUTCMonth() + 1, 0)).getUTCDate();
  return [...Array(offset).fill(null), ...Array.from({length:count}, (_, i) => `${month}-${String(i + 1).padStart(2, "0")}`)];
}

// Availability comes from the catalogue; opening the calendar never scans disks.
export function observationTime({day, button, calendar, time, nowButton, onChange, getDays}) {
  let available = new Set(), live = true, month = utcDay().slice(0, 7);
  const monthLabel = calendar.querySelector("[data-month]"), grid = calendar.querySelector("[data-days]");
  day.value = utcDay(); time.value = utcMinute();
  function update() {
    button.textContent = day.value;
    nowButton.setAttribute("aria-pressed", String(live));
    time.title = live ? "Live UTC time" : "Selected UTC time";
  }
  function draw() {
    monthLabel.textContent = new Date(`${month}-01T00:00:00Z`).toLocaleDateString("en-GB", {month:"long", year:"numeric", timeZone:"UTC"});
    grid.replaceChildren();
    for (const date of calendarDays(month)) {
      if (!date) { grid.append(document.createElement("span")); continue; }
      const b = document.createElement("button");
      b.type = "button"; b.textContent = Number(date.slice(-2)); b.dataset.date = date;
      b.classList.toggle("has-data", available.has(date));
      b.setAttribute("aria-label", `${date}${available.has(date) ? ", data available" : ", no archived data"}`);
      b.setAttribute("aria-pressed", String(date === day.value));
      if (date === utcDay()) b.setAttribute("aria-current", "date");
      b.disabled = date > utcDay() || (!available.has(date) && date !== utcDay());
      b.onclick = () => {
        day.value = date; live = false; update();
        calendar.hidePopover(); button.focus(); onChange();
      };
      grid.append(b);
    }
  }
  button.onclick = () => {
    month = day.value.slice(0, 7); draw();
    const r = button.getBoundingClientRect();
    calendar.style.left = `${Math.max(8, Math.min(r.left, innerWidth - 304))}px`;
    calendar.style.top = `${r.bottom + 6}px`;
    getDays?.().then(days => { available = new Set(days); draw(); }).catch(() => {});
  };
  calendar.addEventListener("toggle", e => button.setAttribute("aria-expanded", String(e.newState === "open")));
  for (const delta of [-1, 1]) calendar.querySelector(`[data-step="${delta}"]`).onclick = () => {
    const d = new Date(`${month}-01T00:00:00Z`); d.setUTCMonth(d.getUTCMonth() + delta);
    month = d.toISOString().slice(0, 7); draw();
  };
  function resume() {
    live = true; day.value = utcDay(); time.value = utcMinute(); update();
    calendar.hidePopover(); onChange();
  }
  calendar.querySelector("[data-today]").onclick = resume;
  nowButton.onclick = resume;
  time.onchange = () => { if (selectionUnix(day.value, time.value) == null) return; live = false; update(); onChange(); };
  update();
  return {
    cutoff: () => live ? null : selectionUnix(day.value, time.value),
    query: () => live ? "" : `&at=${selectionUnix(day.value, time.value)}`,
    label: () => `${day.value} ${live ? "Now" : time.value} UTC`,
    setDays: days => { available = new Set(days); draw(); },
    tick: () => {
      if (!live || document.activeElement === time) return;
      const changedDay = day.value !== utcDay();
      day.value = utcDay(); time.value = utcMinute(); update();
      if (changedDay) onChange();
    },
  };
}
