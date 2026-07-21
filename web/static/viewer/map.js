// MapLibre GL JS wiring (loaded globally via CDN in index.html). Satellite
// basemap is Esri World Imagery — free, no API key, reused exactly from
// the reference Open DroneLog project (src/lib/mapStyles.ts).

const SATELLITE_TILE_URL =
  "https://services.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}";

export function initMap(containerId) {
  return new maplibregl.Map({
    container: containerId,
    style: {
      version: 8,
      sources: {
        satellite: {
          type: "raster",
          tiles: [SATELLITE_TILE_URL],
          tileSize: 256,
          maxzoom: 18,
          attribution: "© Esri",
        },
      },
      layers: [{ id: "satellite", type: "raster", source: "satellite" }],
    },
    center: [-98.5, 39.8],
    zoom: 3,
  });
}

/** Adds or updates a flight's line layer. coordinates: [[lng,lat,alt],...]. */
export function setFlightLayer(map, id, coordinates, color = "#0080ff") {
  const geojson = {
    type: "Feature",
    geometry: { type: "LineString", coordinates: coordinates.map(([lng, lat]) => [lng, lat]) },
  };
  if (map.getSource(id)) {
    map.getSource(id).setData(geojson);
  } else {
    map.addSource(id, { type: "geojson", data: geojson });
    map.addLayer({
      id,
      type: "line",
      source: id,
      layout: { "line-join": "round", "line-cap": "round" },
      paint: { "line-color": color, "line-width": 4 },
    });
  }
}

export function removeFlightLayer(map, id) {
  if (map.getLayer(id)) map.removeLayer(id);
  if (map.getSource(id)) map.removeSource(id);
}

/** Fits the map view to the union of the given coordinate lists. */
export function fitToCoordinates(map, coordinateLists) {
  const all = coordinateLists.flat();
  if (all.length === 0) return;
  let minLng = Infinity, minLat = Infinity, maxLng = -Infinity, maxLat = -Infinity;
  for (const [lng, lat] of all) {
    if (lng < minLng) minLng = lng;
    if (lng > maxLng) maxLng = lng;
    if (lat < minLat) minLat = lat;
    if (lat > maxLat) maxLat = lat;
  }
  map.fitBounds(
    [
      [minLng, minLat],
      [maxLng, maxLat],
    ],
    { padding: 60, duration: 500 },
  );
}
