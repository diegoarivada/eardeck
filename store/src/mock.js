/* Stands in for the Tauri backend so the real popup UI can be rendered with sample data
   for Store screenshots. Loaded before ui/app.js in the generated popup page only. */
(() => {
  const shot = new URLSearchParams(location.search).get("shot") || "home";

  const pro = { id: "pro", name: "AirPods Pro", connected: true };
  const max = { id: "max", name: "AirPods Max", connected: false };

  const status = {
    home: {
      found: true, id: "pro", name: "AirPods Pro", connected: true, is_default: true, worn: true, volume: 62,
      reading: { form: "buds", left: 85, right: 90, case: 60, case_charging: true },
      devices: [pro, max],
    },
    max: {
      found: true, id: "max", name: "AirPods Max", connected: true, is_default: true, worn: true, volume: 48,
      reading: { form: "single", main: 72 },
      devices: [{ ...pro, connected: false }, { ...max, connected: true }],
    },
  };
  const s = status[shot === "max" ? "max" : "home"];

  const listeners = {};
  window.__TAURI__ = {
    core: {
      invoke: async (cmd) => {
        if (cmd === "get_status") return s;
        if (cmd === "get_settings") return { auto_pause: true, auto_default: true, popup_on_connect: true, low_battery: 20 };
        if (cmd === "get_autostart") return true;
        if (cmd === "start_pair_scan") {
          setTimeout(() => listeners["pair-found"]?.({ payload: { id: "new", name: "AirPods Pro" } }), 50);
        }
        return null;
      },
    },
    event: { listen: async (name, fn) => { listeners[name] = fn; } },
  };

  /* Put the UI into the state each screenshot shows once app.js has wired it up. */
  window.addEventListener("load", () => setTimeout(() => {
    if (shot === "devices") document.getElementById("open-devices").click();
    if (shot === "pair") {
      document.getElementById("open-devices").click();
      document.getElementById("open-pair").click();
    }
  }, 50));
})();
