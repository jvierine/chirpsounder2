// Browser presentation preferences; science arrays and their resolution stay unchanged.
export function figureConfig(value = {}) {
  const number = (name, low, high) => {
    const v = value?.[name];
    return typeof v === "number" && Number.isFinite(v) && v >= low && v <= high ? v : null;
  };
  return {width:number("width",320,2400), height:number("height",240,1600), fontSize:number("fontSize",10,32)};
}
let expanded = null;
export function panelControls(panel, toolbar, label, changed) {
  const buttons = [document.createElement("button"), document.createElement("button")];
  let minimized = false;
  const update = (notify = false) => {
    const maximized = expanded === panel;
    panel.classList.toggle("panel-minimized", minimized);
    panel.classList.toggle("panel-maximized", maximized);
    document.body.classList.toggle("has-maximized-panel", !!expanded);
    buttons[0].textContent = minimized ? "Show" : "−";
    buttons[0].title = buttons[0].ariaLabel = `${minimized ? "Show" : "Minimize"} ${label()}`;
    buttons[0].setAttribute("aria-expanded", String(!minimized));
    buttons[1].textContent = maximized ? "Restore" : "⛶";
    buttons[1].title = buttons[1].ariaLabel = `${maximized ? "Restore" : "Maximize"} ${label()}`;
    buttons[1].setAttribute("aria-expanded", String(maximized));
    if (notify) changed();
  };
  const restore = () => { if (expanded === panel) { expanded = null; update(true); } };
  buttons[0].onclick = () => { if (expanded === panel) expanded = null; minimized = !minimized; update(true); };
  buttons[1].onclick = () => {
    if (expanded && expanded !== panel) expanded.dispatchEvent(new Event("panel-restore"));
    expanded = expanded === panel ? null : panel;
    minimized = false;
    update(true);
  };
  panel.addEventListener("panel-restore", restore);
  toolbar.append(...buttons);
  return {update, dispose:restore};
}
document.addEventListener("keydown", e => {
  if (e.key === "Escape" && expanded && !document.querySelector("dialog[open]"))
    expanded.dispatchEvent(new Event("panel-restore"));
});
