// Unzips a .kmz and parses its doc.kml — only ever needs to understand
// DJI2KMZ's own known, simple output shape (core/src/kml.rs), not
// arbitrary third-party KML. JSZip (loaded globally via CDN in
// index.html) is the right fit here: this only ever needs "open a zip,
// read one known entry as a string," and files are read on-demand per
// clicked folder, never preloaded, so its size is a one-time,
// non-blocking cost.

export async function readKmlFromKmz(file) {
  const zip = await JSZip.loadAsync(await file.arrayBuffer());
  const entry = zip.file("doc.kml");
  if (!entry) throw new Error(`${file.name}: no doc.kml entry found`);
  return entry.async("string");
}

const FIELD_MAP = {
  "Drone Model": "droneModel",
  "Aircraft Serial": "aircraftSerial",
  "Aircraft Name": "aircraftName",
  Pilot: "pilot",
  "Battery Serial": "batterySerial",
  "Start Time": "startTime",
  Duration: "duration",
  Distance: "distance",
  "Max Altitude": "maxAltitude",
  "Max Speed": "maxSpeed",
};

function parseDescription(text) {
  const meta = {};
  for (const line of text.split("\n")) {
    const idx = line.indexOf(":");
    if (idx === -1) continue;
    const key = FIELD_MAP[line.slice(0, idx).trim()];
    if (key) meta[key] = line.slice(idx + 1).trim();
  }
  return meta;
}

function parsePlacemark(pmNode) {
  const name = pmNode.getElementsByTagName("name")[0]?.textContent?.trim() ?? "Unnamed Flight";
  const meta = parseDescription(pmNode.getElementsByTagName("description")[0]?.textContent ?? "");
  const coordsText = pmNode.getElementsByTagName("coordinates")[0]?.textContent ?? "";
  const coordinates = coordsText
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .map((triplet) => triplet.split(",").map(Number)); // [lng, lat, alt]
  return { name, meta, coordinates };
}

/**
 * Parses raw KML text into one entry per <Placemark>. Uses
 * "application/xml" (not "text/xml") so malformed input reliably
 * surfaces as a <parsererror> node rather than a silent partial tree —
 * acceptable since this page is already Chromium-only by design.
 * element.textContent already concatenates CDATA child text, so no
 * manual "]]>" stripping is needed for the description field.
 */
export function parseKml(kmlText) {
  const doc = new DOMParser().parseFromString(kmlText, "application/xml");
  const err = doc.querySelector("parsererror");
  if (err) throw new Error("Malformed KML: " + err.textContent);
  return Array.from(doc.getElementsByTagName("Placemark")).map(parsePlacemark);
}
