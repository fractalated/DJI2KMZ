// Groups collected .kmz files by folder and sorts by date — all derived
// from filenames alone (DJI2KMZ's own naming contract), so building the
// sidebar never requires opening/parsing any file content. Matches
// core/src/naming.rs exactly:
//   individual: "{MM-DD-YYYY}_{HH-MM}_{cleaned_folder_name}.kmz"
//   merged:     "{cleaned_folder_name}_Flight_Logs_{MM-DD-YYYY}.kmz"
//           or  "{cleaned_folder_name}_Flight_Logs_{MM-DD--MM-DD-YYYY}.kmz"
// In both merged shapes the date immediately before ".kmz" is always a
// full MM-DD-YYYY, so one regex covers both.

import { readKmlFromKmz, parseKml } from "./kml.js";

const INDIVIDUAL_PREFIX_RE = /^(\d{2})-(\d{2})-(\d{4})_/;
const MERGED_SUFFIX_RE = /(\d{2})-(\d{2})-(\d{4})\.kmz$/i;

// clean_folder_name (core/src/naming.rs) strips "flight"/"flights"/
// "log"/"logs" from folder names, so a legitimate cleaned folder name can
// never itself contain this substring — safe, unambiguous marker.
export function isMerged(name) {
  return name.includes("_Flight_Logs_");
}

/**
 * Sortable "YYYYMMDD" from any name following this project's naming
 * contract — an individual filename, a merged filename, or (this is why
 * it's exported) a placemark's `<name>`, which is always the individual
 * filename format even inside a merged KMZ. This is the LOCAL date
 * embedded in the original filename, not the UTC date in a KML
 * description's "Start Time" — the two can differ by a day depending on
 * timezone, and callers wanting the pilot's actual local flight date
 * (e.g. the logbook) must use this, not the description.
 */
export function dateKeyFromFilename(name) {
  const m = isMerged(name) ? name.match(MERGED_SUFFIX_RE) : name.match(INDIVIDUAL_PREFIX_RE);
  if (!m) return null;
  const [, mm, dd, yyyy] = m;
  return `${yyyy}${mm}${dd}`; // sortable YYYYMMDD
}

/**
 * Parses a date directly out of a folder's own name, as a fallback for
 * when nothing inside it has a filename DJI2KMZ's own regex recognizes
 * (e.g. a folder organized before this project's naming conventions
 * existed). Recognizes this project's own `YYYY-MM-DD` destination
 * folders, plus common legacy shapes like `6-15-26` or `06-15-2026`
 * (2-digit years assumed 20xx). Returns a sortable "YYYYMMDD", or `null`
 * if the name doesn't look like a date at all.
 */
export function parseDateFromFolderName(name) {
  let m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(name);
  if (m) return `${m[1]}${m[2]}${m[3]}`;

  m = /^(\d{1,2})-(\d{1,2})-(\d{2}|\d{4})$/.exec(name);
  if (m) {
    const [, mm, dd, yy] = m;
    const yyyy = yy.length === 2 ? `20${yy}` : yy;
    return `${yyyy}${mm.padStart(2, "0")}${dd.padStart(2, "0")}`;
  }

  return null;
}

/**
 * entries: [{file, folderKey, folderName}] from collectKmzFiles.
 * Returns sidebar-ready location entries, sorted newest-first.
 */
export function buildLocationEntries(entries) {
  const byFolder = new Map();
  for (const e of entries) {
    if (!byFolder.has(e.folderKey)) {
      byFolder.set(e.folderKey, { folderName: e.folderName, files: [] });
    }
    byFolder.get(e.folderKey).files.push(e.file);
  }

  const locations = [];
  for (const [folderKey, { folderName, files }] of byFolder) {
    const merged = files.find((f) => isMerged(f.name)) ?? null;
    // The flight log's own date (from its filename, the cheapest available
    // proxy for its content — no need to open/parse the file just to
    // group the sidebar) takes priority; the containing folder's name is
    // only a fallback for names this project's own regex doesn't
    // recognize, so a folder that's plainly named e.g. "6-15-26" doesn't
    // show up as "Unknown date" when a clearly-encoded date is right there.
    const dateKey =
      (merged
        ? dateKeyFromFilename(merged.name)
        : files
            .map((f) => dateKeyFromFilename(f.name))
            .filter(Boolean)
            .sort()
            .at(-1)) ?? parseDateFromFolderName(folderName);

    locations.push({
      folderKey,
      folderName,
      dateKey: dateKey ?? "00000000", // unrecognized names sink to one end, never crash the sort
      mergedFile: merged,
      individualFiles: merged ? [] : files, // only used as the fallback path
    });
  }

  locations.sort((a, b) => b.dateKey.localeCompare(a.dateKey));
  return locations;
}

/** "20260615" -> "06/15/2026", for sidebar display. */
export function formatDateKey(dateKey) {
  if (!dateKey || dateKey === "00000000") return "Unknown date";
  const yyyy = dateKey.slice(0, 4);
  const mm = dateKey.slice(4, 6);
  const dd = dateKey.slice(6, 8);
  return `${mm}/${dd}/${yyyy}`;
}

/** "2026-06-15" -> "06/15/2026", for sidebar display. */
export function formatDateFolder(dateFolder) {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(dateFolder);
  if (!m) return dateFolder;
  const [, yyyy, mm, dd] = m;
  return `${mm}/${dd}/${yyyy}`;
}

/**
 * Groups `buildLocationEntries`'s flat per-folder list into a
 * Project -> Date tree, for the viewer's destination-folder layout
 * (`KMZs/{project}/{date}/*.kmz` — the viewer is pointed at `KMZs`
 * itself, so a location's `folderKey` there is `{project}/{date}`). The
 * date comes straight from the folder name (an explicit `YYYY-MM-DD`
 * directory, not something parsed out of filenames), which is also more
 * reliable than filename parsing.
 *
 * A location with only one path segment doesn't fit this shape — e.g.
 * `.kmz` files converted under the old flat convention, or the viewer
 * pointed at the wrong folder — those fall into `ungrouped` instead of
 * silently disappearing.
 */
export function buildProjectTree(entries) {
  const locations = buildLocationEntries(entries);

  const byProject = new Map();
  const ungrouped = [];

  for (const loc of locations) {
    const segments = loc.folderKey.split("/");
    if (segments.length < 2) {
      ungrouped.push(loc);
      continue;
    }
    const projectName = segments[0];
    const dateFolder = segments[1];
    if (!byProject.has(projectName)) {
      byProject.set(projectName, { projectName, dates: [] });
    }
    byProject.get(projectName).dates.push({ ...loc, dateFolder });
  }

  const projects = Array.from(byProject.values());
  for (const project of projects) {
    project.dates.sort((a, b) => b.dateFolder.localeCompare(a.dateFolder)); // newest first
  }
  projects.sort((a, b) => a.projectName.localeCompare(b.projectName));

  return { projects, ungrouped };
}

// folderKey -> Promise<placemark[]>. Module-scoped, so each page (viewer,
// logbook) gets its own fresh cache on load — caching the in-flight
// promise (not just the resolved value) dedupes a rapid double-load of
// the same folder before its first read finishes.
const flightCache = new Map();

/**
 * Loads every placemark for a location entry (from `buildLocationEntries`)
 * — its merged KMZ if present (one file, fast), or every individual KMZ
 * in that folder as a fallback. Shared between the viewer (renders each
 * placemark's coordinates on the map) and the logbook (reads only the
 * metadata/name, discards coordinates) so "which files represent this
 * location's flights" is derived in exactly one place.
 */
export function loadPlacemarks(entry) {
  if (!flightCache.has(entry.folderKey)) {
    const files = entry.mergedFile ? [entry.mergedFile] : entry.individualFiles;
    flightCache.set(
      entry.folderKey,
      (async () => {
        const placemarks = [];
        for (const file of files) {
          placemarks.push(...parseKml(await readKmlFromKmz(file)));
        }
        return placemarks;
      })(),
    );
  }
  return flightCache.get(entry.folderKey);
}
