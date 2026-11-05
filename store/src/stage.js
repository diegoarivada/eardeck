const shots = {
  home: ["Your AirPods battery, right by the clock", "See left, right and case at a glance, connect in one click, and set the volume."],
  pair: ["Pair new AirPods in seconds", "Open the case, hold the button, and click Connect. No digging through Windows settings."],
  devices: ["Switch between your pairs", "Keep more than one pair of AirPods or Beats and pick the one you're using."],
  max: ["Works with AirPods Max too", "AirPods, AirPods Pro, AirPods Max and several Beats models."],
};
const shot = new URLSearchParams(location.search).get("shot") || "home";
const [title, text] = shots[shot];
document.getElementById("title").textContent = title;
document.getElementById("text").textContent = text;
document.getElementById("frame").src = `popup.html?shot=${shot}`;
