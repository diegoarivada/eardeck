const tauri = window.__TAURI__;
const invoke = (cmd, args) => tauri.core.invoke(cmd, args);
const $ = (id) => document.getElementById(id);
const esc = (s) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));

const SCAN_MS = 30000;

const OVER_EAR = `<svg viewBox="0 0 120 120" aria-hidden="true">
  <defs>
    <linearGradient id="m" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="var(--metal-a)"/><stop offset="1" stop-color="var(--metal-b)"/></linearGradient>
  </defs>
  <path d="M26 60V52a34 34 0 0 1 68 0v8" fill="none" stroke="url(#m)" stroke-width="6" stroke-linecap="round"/>
  <path d="M34 34a32 32 0 0 1 52 0" fill="none" stroke="var(--metal-line)" stroke-width="1.500" stroke-dasharray="1.500 2.500" stroke-linecap="round"/>
  <rect x="23" y="55" width="6" height="10" rx="2" fill="var(--metal-line)"/>
  <rect x="91" y="55" width="6" height="10" rx="2" fill="var(--metal-line)"/>
  <rect x="12" y="63" width="28" height="40" rx="11" fill="url(#m)"/>
  <rect x="80" y="63" width="28" height="40" rx="11" fill="url(#m)"/>
  <rect x="30" y="69" width="10" height="28" rx="5" fill="var(--metal-line)" opacity=".7"/>
  <rect x="80" y="69" width="10" height="28" rx="5" fill="var(--metal-line)" opacity=".7"/>
</svg>`;

const EARBUDS = `<svg viewBox="0 0 120 120" aria-hidden="true">
  <defs>
    <linearGradient id="m" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="var(--metal-a)"/><stop offset="1" stop-color="var(--metal-b)"/></linearGradient>
  </defs>
  <g fill="url(#m)">
    <path d="M34 22c10 0 17 8 17 17 0 6-3 10-6 13v42a6 6 0 0 1-12 0V58c-9-1-16-9-16-18 0-10 8-18 17-18Z"/>
    <path d="M86 22c-10 0-17 8-17 17 0 6 3 10 6 13v42a6 6 0 0 0 12 0V58c9-1 16-9 16-18 0-10-8-18-17-18Z"/>
  </g>
  <circle cx="30" cy="38" r="6" fill="var(--metal-line)" opacity=".7"/>
  <circle cx="90" cy="38" r="6" fill="var(--metal-line)" opacity=".7"/>
</svg>`;

const CHECK = `<svg class="check" viewBox="0 0 24 24"><path d="M5 13l5 5 9-11"/></svg>`;

function battery(label, level, charging) {
  if (level === null || level === undefined) return "";
  const cls = ["battery", charging && "charging", level <= 20 && !charging && "low"].filter(Boolean).join(" ");
  const tag = label ? `<small>${label}</small>` : "";
  return `<div class="${cls}">${tag}<span class="cell"><span data-level="${level}"></span></span>${level}%</div>`;
}

/* Inline style attributes are blocked by the CSP, so widths are set from script. */
function fillBatteries() {
  for (const bar of document.querySelectorAll("[data-level]")) bar.style.width = `${bar.dataset.level}%`;
}

function subtitle(s) {
  if (!s.found) return "Add a pair with the + button";
  if (s.worn === true) return s.connected ? "Connected" : "Being worn, not connected to this PC";
  if (s.connected) return "Connected";
  if (s.seen_secs_ago != null) return s.worn === false ? "Nearby" : `Last seen ${Math.max(1, Math.round(s.seen_secs_ago / 60))} min ago`;
  return "Not connected";
}

let dragging = false;
let current = null;

function setSlider(value) {
  $("volume").value = value;
  $("volume").style.setProperty("--pct", `${value}%`);
}

function renderDevices(s) {
  $("device-list").innerHTML = s.devices.map((d) => `
    <button class="row action device-row" data-id="${esc(d.id)}">
      <span class="name">${esc(d.name)}<small>${d.connected ? "Connected" : "Not connected"}</small></span>
      ${d.id === s.id ? CHECK : ""}
    </button>`).join("");
}

function render(s) {
  current = s;
  const r = s.reading;
  const single = r ? r.form === "single" : /max|studio|solo/i.test(s.name);
  $("art").innerHTML = single ? OVER_EAR : EARBUDS;
  $("name").textContent = s.found ? s.name : "No AirPods Yet";
  $("sub").textContent = subtitle(s);

  $("batteries").innerHTML = single
    ? battery("", r?.main, r?.main_charging)
    : battery("L", r?.left, r?.left_charging) + battery("R", r?.right, r?.right_charging) + battery("Case", r?.case, r?.case_charging);
  fillBatteries();

  const c = $("connect");
  c.textContent = s.connected ? "Disconnect" : "Connect";
  c.classList.toggle("quiet", s.connected);
  c.disabled = !s.found;

  $("output").disabled = !s.connected;
  $("output").classList.toggle("current", s.is_default);
  $("output-value").textContent = !s.connected ? "Not active" : s.is_default ? "This Device" : "Switch";

  $("volume-row").classList.toggle("off", !s.connected);
  if (!dragging && s.volume != null) setSlider(s.volume);
  renderDevices(s);
}

async function run(cmd, args) {
  $("error").hidden = true;
  try {
    return await invoke(cmd, args);
  } catch (e) {
    $("error").textContent = String(e);
    $("error").hidden = false;
    throw e;
  }
}

/* ---------- pairing card ---------- */

let scanTimer = null;

function sheetState({ title, text, art, spinning, action, settings }) {
  $("sheet-title").textContent = title;
  $("sheet-text").textContent = text ?? "";
  $("sheet-text").hidden = !text;
  $("sheet-art").innerHTML = art ?? "";
  $("sheet-art").hidden = !art;
  $("spinner").hidden = !spinning;
  $("sheet-action").hidden = !action;
  $("sheet-action").textContent = action ?? "";
  $("sheet-settings").hidden = !settings;
}

function showSearching() {
  clearTimeout(scanTimer);
  $("found").innerHTML = "";
  sheetState({
    title: "Connect New AirPods",
    text: "Open the case and hold the button on the back until the light flashes white. On AirPods Max, hold the noise control button.",
    spinning: true,
  });
  sheetAction = null;
  if (tauri) {
    invoke("start_pair_scan").catch((e) => showFailed(String(e)));
    scanTimer = setTimeout(() => {
      invoke("stop_pair_scan");
      if (!$("found").children.length) showFailed("Nothing showed up. Check that the light is flashing white, then try again.");
      else $("spinner").hidden = true;
    }, SCAN_MS);
  }
}

function showFailed(text) {
  clearTimeout(scanTimer);
  $("found").innerHTML = "";
  sheetState({ title: "Couldn't Connect", text, action: "Try Again", settings: true });
  sheetAction = showSearching;
}

function addFound(device) {
  const list = $("found");
  if ([...list.children].some((el) => el.dataset.id === device.id)) return;
  const row = document.createElement("button");
  row.className = "row action";
  row.dataset.id = device.id;
  row.innerHTML = `<span class="name">${esc(device.name)}</span><em>Connect</em>`;
  row.addEventListener("click", () => pair(device));
  list.append(row);
}

async function pair(device) {
  clearTimeout(scanTimer);
  $("found").innerHTML = "";
  const art = /max|studio|solo/i.test(device.name) ? OVER_EAR : EARBUDS;
  sheetState({ title: device.name, art, spinning: true });
  $("spinner").lastChild.textContent = "Connecting";
  try {
    await invoke("pair_device", { id: device.id, name: device.name });
    sheetState({ title: device.name, text: "Connected. They're ready to use.", art, action: "Done" });
    sheetAction = closeSheet;
  } catch (e) {
    showFailed(String(e));
  } finally {
    $("spinner").lastChild.textContent = "Looking nearby";
  }
}

function openSheet() {
  $("sheet").classList.add("up");
  $("scrim").classList.add("up");
  showSearching();
}

function closeSheet() {
  clearTimeout(scanTimer);
  $("sheet").classList.remove("up");
  $("scrim").classList.remove("up");
  if (tauri) invoke("stop_pair_scan");
  if ($("sheet-action").textContent === "Done") $("pages").classList.remove("pushed");
}

let sheetAction = null;

/* ---------- wiring ---------- */

$("open-devices").addEventListener("click", () => $("pages").classList.add("pushed"));
$("close-devices").addEventListener("click", () => $("pages").classList.remove("pushed"));
$("open-pair").addEventListener("click", openSheet);
$("close-pair").addEventListener("click", closeSheet);
$("scrim").addEventListener("click", closeSheet);
$("sheet-action").addEventListener("click", () => sheetAction?.());

async function init() {
  render(await invoke("get_status"));
  const settings = await invoke("get_settings");
  for (const key of ["auto_pause", "auto_default", "popup_on_connect"]) {
    $(key).checked = settings[key];
    $(key).addEventListener("change", () => {
      settings[key] = $(key).checked;
      run("save_settings", { settings });
    });
  }
  $("autostart").checked = await invoke("get_autostart");
  $("autostart").addEventListener("change", () =>
    run("set_autostart", { enabled: $("autostart").checked }));

  $("connect").addEventListener("click", () =>
    run(current?.connected ? "disconnect" : "connect").catch(() => {}));
  $("output").addEventListener("click", () => {
    if (!current?.is_default) run("make_default").catch(() => {});
  });
  $("quit").addEventListener("click", () => run("quit"));
  $("sheet-settings").addEventListener("click", () => invoke("open_bluetooth_settings"));

  $("device-list").addEventListener("click", (e) => {
    const row = e.target.closest("[data-id]");
    if (row) {
      run("select_device", { id: row.dataset.id }).catch(() => {});
      $("pages").classList.remove("pushed");
    }
  });

  const slider = $("volume");
  slider.addEventListener("pointerdown", () => { dragging = true; });
  slider.addEventListener("pointerup", () => { dragging = false; });
  slider.addEventListener("input", () => {
    setSlider(slider.value);
    run("set_volume", { percent: Number(slider.value) }).catch(() => {});
  });

  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    if ($("sheet").classList.contains("up")) closeSheet();
    else if ($("pages").classList.contains("pushed")) $("pages").classList.remove("pushed");
    else run("hide_popup");
  });

  document.addEventListener("contextmenu", (e) => e.preventDefault());

  tauri.event.listen("status", (e) => render(e.payload));
  tauri.event.listen("pair-found", (e) => addFound(e.payload));
  invoke("ui_ready");
}

if (tauri) init();
