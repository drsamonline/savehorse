//! One implementation of every ported operation.
//!
//! The command line and the desktop app both call [`execute`]. Neither owns the
//! logic, so a fix or a behaviour change lands in both at once and they cannot
//! drift apart.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use savehorse_core::{Error, Result};
use savehorse_registry as registry;

/// A single request: which operation, and with what.
#[derive(Debug, Clone, Default)]
pub struct Request {
    pub module: String,
    pub op: String,
    /// Positional arguments, in the order the manifest declares them.
    pub positional: Vec<String>,
    /// Flags by name, without leading dashes.
    pub flags: BTreeMap<String, String>,
}

impl Request {
    /// Build a request for `module.op`.
    pub fn new(module: &str, op: &str) -> Self {
        Request {
            module: module.to_string(),
            op: op.to_string(),
            positional: Vec::new(),
            flags: BTreeMap::new(),
        }
    }

    /// Chainable positional argument.
    pub fn arg(mut self, value: impl Into<String>) -> Self {
        self.positional.push(value.into());
        self
    }

    /// Chainable flag.
    pub fn flag(mut self, name: &str, value: impl Into<String>) -> Self {
        self.flags.insert(name.to_string(), value.into());
        self
    }

    /// True when a switch is present and not explicitly off.
    pub fn has(&self, name: &str) -> bool {
        match self.flags.get(name) {
            None => false,
            Some(v) => v != "false" && v != "0",
        }
    }

    pub fn value(&self, name: &str) -> Option<&str> {
        self.flags.get(name).map(|s| s.as_str())
    }

    fn position(&self, index: usize, what: &str) -> Result<&str> {
        self.positional
            .get(index)
            .map(|s| s.as_str())
            .ok_or_else(|| Error::Invalid(format!("{} is required", what)))
    }

    fn number(&self, name: &str) -> Result<Option<usize>> {
        match self.value(name) {
            None => Ok(None),
            Some(raw) if raw.trim().is_empty() => Ok(None),
            Some(raw) => raw.trim().parse::<usize>().map(Some).map_err(|_| {
                Error::Invalid(format!("--{} wants a whole number, got '{}'", name, raw))
            }),
        }
    }

    fn size(&self, name: &str) -> Result<Option<u64>> {
        match self.value(name) {
            None => Ok(None),
            Some(raw) if raw.trim().is_empty() => Ok(None),
            Some(raw) => savehorse_core::parse_size(raw).map(Some),
        }
    }

    fn paths_from(&self, from: usize) -> Result<Vec<PathBuf>> {
        let paths: Vec<PathBuf> = self.positional.iter().skip(from).map(PathBuf::from).collect();
        if paths.is_empty() {
            return Err(Error::Invalid("at least one path is required".to_string()));
        }
        Ok(paths)
    }
}

/// What an operation produced.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// A report written for a person.
    pub text: String,
    /// The same result as JSON, for a machine or another pane.
    pub json: String,
    /// True when the operation was a preview and changed nothing.
    pub dry_run: bool,
}

impl Outcome {
    fn new(text: String, json: String) -> Self {
        Outcome {
            text,
            json,
            dry_run: false,
        }
    }

    fn preview(mut self) -> Self {
        self.dry_run = true;
        self
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string_pretty(value)
        .map_err(|e| Error::Invalid(format!("could not encode JSON: {}", e)))
}

/// Look up the operation and run it.
///
/// An operation that is declared but not ported returns an error naming itself,
/// rather than doing nothing quietly.
pub fn execute(req: &Request) -> Result<Outcome> {
    let op = registry::op(&req.module, &req.op).ok_or_else(|| {
        Error::Invalid(format!(
            "module '{}' has no operation '{}'",
            req.module, req.op
        ))
    })?;

    if op.status == registry::Status::Planned {
        return Err(Error::Invalid(format!(
            "{} is declared in the manifest but not ported yet",
            op.id
        )));
    }

    match op.id {
        "sift.strings" => sift_strings(req),
        "sift.search" => sift_search(req),
        "sift.replace" => sift_replace(req),
        "sift.xor" => sift_xor(req),
        "scout.catalog" => scout_catalog(req),
        "scout.duplicates" => scout_duplicates(req),
        "scout.clean" => scout_clean(req),
        "glyph.pdf" => glyph_pdf(req),
        "glyph.extract" => glyph_extract(req),
        "glyph.markdown" => glyph_markdown(req),
        "glyph.encoding" => glyph_encoding(req),
        "glyph.readability" => glyph_readability(req),
        "glyph.metrics" => glyph_metrics(req),
        "halo.create" => halo_create(req),
        "halo.list" => halo_list(req),
        "halo.extract" => halo_extract(req),
        "halo.verify" => halo_verify(req),
        "halo.convert" => halo_convert(req),
        "iris.info" => iris_info(req),
        "iris.convert" => iris_convert(req),
        "iris.thumb" => iris_thumb(req),
        "iris.transform" => iris_transform(req),
        "iris.stats" => iris_stats(req),
        "iris.batch" => iris_batch(req),
        "arc.create" => arc_create(req),
        "arc.extract" => arc_extract(req),
        "arc.list" => arc_list(req),
        "arc.test" => arc_test(req),
        "arc.split" => arc_split(req),
        "arc.join" => arc_join(req),
        "revive.scan" => revive_scan(req),
        "revive.carve" => revive_carve(req),
        "revive.report" => revive_report(req),
        "revive.pack" => revive_pack(req),
        "revive.unpack" => revive_unpack(req),
        "revive.verify" => revive_verify(req),
        "revive.geometry" => revive_geometry(req),
        "sift.view" => sift_view(req),
        "sift.eol" => sift_eol(req),
        "sift.clean" => sift_clean(req),
        "sift.stats" => sift_stats(req),
        "sift.convert" => sift_convert(req),
        "forge.inspect" => forge_inspect(req),
        "forge.sections" => forge_sections(req),
        "forge.imports" => forge_imports(req),
        "forge.hexdump" => forge_hexdump(req),
        "forge.search" => forge_search(req),
        "forge.patch" => forge_patch(req),
        "morph.rename" => morph_rename(req),
        "morph.undo" => morph_undo(req),
        "aegis.encrypt" => aegis_encrypt(req),
        "aegis.decrypt" => aegis_decrypt(req),
        "aegis.shred" => aegis_shred(req),
        "aegis.scan" => aegis_scan(req),
        "aegis.harden" => aegis_harden(req),
        "pulse.hash" => pulse_hash(req),
        "pulse.temp" => pulse_temp(req),
        "halo.cue" => halo_cue(req),
        "halo.cover" => halo_cover(req),
        "iris.ico" => iris_ico(req),
        "iris.sheet" => iris_sheet(req),
        "arc.chunk" => arc_chunk(req),
        "arc.torrent" => arc_torrent(req),
        "arc.probe" => arc_probe(req),
        "arc.upnp" => arc_upnp(req),
        "scout.treemap" => scout_treemap(req),
        "scout.inventory" => scout_inventory(req),
        "scout.diff" => scout_diff(req),
        "aegis.persistence" => aegis_persistence(req),
        "flux.inspect" => flux_inspect(req),
        "flux.synth" => flux_synth(req),
        "flux.tag" => flux_tag(req),
        "flux.gain" => flux_gain(req),
        other => Err(Error::Invalid(format!(
            "{} is marked implemented but has no runner",
            other
        ))),
    }
}

// ---------------------------------------------------------------------------
// sift
// ---------------------------------------------------------------------------

fn sift_strings(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let min_len = req.number("min-len")?.unwrap_or(4);
    let entries = savehorse_sift::extract_strings(
        &path,
        min_len,
        req.has("ascii"),
        req.has("unicode"),
    )?;

    let mut text = String::new();
    for e in &entries {
        text.push_str(&format!("{}  {:<8}  {}\n", e.offset_hex, e.encoding, e.content));
    }
    text.push_str(&format!("{} string(s)\n", entries.len()));

    Ok(Outcome::new(text, to_json(&entries)?))
}

fn sift_search(req: &Request) -> Result<Outcome> {
    let pattern = req.position(0, "pattern")?;
    let paths = req.paths_from(1)?;

    let hits = savehorse_sift::search_paths(
        pattern,
        &paths,
        req.has("regex"),
        req.has("ignore-case"),
        req.value("glob"),
    )?;

    let mut text = String::new();
    for h in &hits {
        text.push_str(&format!("{}:{}: {}\n", h.file, h.line_number, h.content));
    }
    text.push_str(&format!("{} match(es)\n", hits.len()));

    Ok(Outcome::new(text, to_json(&hits)?))
}

fn sift_replace(req: &Request) -> Result<Outcome> {
    let pattern = req.position(0, "pattern")?;
    let replacement = req.position(1, "replacement")?;
    let paths = req.paths_from(2)?;
    let dry_run = req.has("dry-run");

    let outcome = savehorse_sift::replace_paths(
        pattern,
        replacement,
        &paths,
        req.has("ignore-case"),
        dry_run,
        req.has("backup"),
        req.value("glob"),
    )?;

    let mut text = String::new();
    for line in &outcome.log {
        text.push_str(line);
        text.push('\n');
    }
    if outcome.dry_run {
        text.push_str(&format!(
            "preview: {} replacement(s) in {} file(s). Re-run without --dry-run to apply.\n",
            outcome.replacements, outcome.files_modified
        ));
    } else {
        text.push_str(&format!(
            "{} replacement(s) in {} file(s).\n",
            outcome.replacements, outcome.files_modified
        ));
    }

    let mut result = Outcome::new(text, to_json(&outcome)?);
    if dry_run {
        result = result.preview();
    }
    Ok(result)
}

fn sift_xor(req: &Request) -> Result<Outcome> {
    let pattern = req.position(0, "pattern")?;
    let path = PathBuf::from(req.position(1, "path")?);
    let max_key = req.number("max-key")?.unwrap_or(255).min(255) as u8;

    let hits = savehorse_sift::xor_search(pattern, &path, max_key)?;

    let mut text = String::new();
    for h in &hits {
        text.push_str(&format!(
            "key=0x{:02x}  offset=0x{:08x}  {}\n",
            h.key, h.offset, h.context
        ));
    }
    text.push_str(&format!("{} hit(s)\n", hits.len()));

    Ok(Outcome::new(text, to_json(&hits)?))
}

// ---------------------------------------------------------------------------
// scout
// ---------------------------------------------------------------------------

fn scout_catalog(req: &Request) -> Result<Outcome> {
    let root = PathBuf::from(req.position(0, "path")?);
    let format = match req.value("format") {
        Some(f) if !f.trim().is_empty() => savehorse_scout::ExportFormat::parse(f)?,
        _ => savehorse_scout::ExportFormat::Table,
    };

    let opts = savehorse_core::WalkOptions {
        max_depth: req.number("depth")?,
        min_size: req.size("min-size")?,
        max_size: req.size("max-size")?,
        ext: req.value("ext").map(|s| s.to_string()),
        include_hidden: false,
    };

    let entries = savehorse_scout::build_catalog(&root, &opts)?;
    let rendered = savehorse_scout::render_catalog(&entries, format)?;
    let json = to_json(&entries)?;

    if let Some(dest) = req.value("out") {
        if !dest.trim().is_empty() {
            std::fs::write(dest, &rendered)?;
            return Ok(Outcome::new(
                format!("catalogued {} file(s) to '{}'\n", entries.len(), dest),
                json,
            ));
        }
    }

    Ok(Outcome::new(rendered, json))
}

fn scout_duplicates(req: &Request) -> Result<Outcome> {
    let root = PathBuf::from(req.position(0, "path")?);
    let summary = savehorse_scout::find_duplicates(
        &root,
        req.size("min-size")?.unwrap_or(1),
        req.value("ext"),
        req.number("depth")?,
    )?;

    let mut text = format!(
        "Duplicate scan of '{}':\n  clusters           : {}\n  redundant files    : {}\n  recoverable space  : {}\n",
        root.display(),
        summary.total_clusters,
        summary.total_duplicate_files,
        summary.total_wasted_human
    );
    if !summary.clusters.is_empty() {
        text.push('\n');
    }
    for (idx, cluster) in summary.clusters.iter().enumerate() {
        let short = &cluster.sha256[..16.min(cluster.sha256.len())];
        text.push_str(&format!(
            "Cluster #{}: sha256={} ({} each)\n",
            idx + 1,
            short,
            cluster.file_size_human
        ));
        text.push_str(&format!("  keep      {}\n", cluster.original));
        for dup in &cluster.duplicates {
            text.push_str(&format!("  duplicate {}\n", dup));
        }
        text.push_str(&format!(
            "  wasted    {}\n\n",
            savehorse_core::format_bytes(cluster.wasted_bytes)
        ));
    }

    Ok(Outcome::new(text, to_json(&summary)?))
}

fn scout_clean(req: &Request) -> Result<Outcome> {
    let root = PathBuf::from(req.position(0, "path")?);
    let commit = req.has("commit");

    let summary =
        savehorse_scout::find_duplicates(&root, req.size("min-size")?.unwrap_or(1), req.value("ext"), None)?;
    let outcome = savehorse_scout::delete_duplicates(&summary, commit);

    let mut text = String::new();
    if !commit {
        text.push_str("Preview: nothing has been deleted.\n");
        for path in &outcome.removed {
            text.push_str(&format!("  would remove {}\n", path));
        }
        text.push_str(&format!(
            "\n{} file(s), {} reclaimable. Re-run with --commit to delete.\n",
            outcome.removed_count, outcome.reclaimed_human
        ));
    } else {
        for path in &outcome.removed {
            text.push_str(&format!("  removed {}\n", path));
        }
        text.push_str(&format!(
            "\n{} file(s) removed, {} reclaimed.\n",
            outcome.removed_count, outcome.reclaimed_human
        ));
    }

    let mut result = Outcome::new(text, to_json(&outcome)?);
    if !commit {
        result = result.preview();
    }
    Ok(result)
}


// ---------------------------------------------------------------------------
// glyph
// ---------------------------------------------------------------------------

fn required_value<'a>(req: &'a Request, name: &str) -> Result<&'a str> {
    match req.value(name) {
        Some(v) if !v.trim().is_empty() => Ok(v),
        _ => Err(Error::Invalid(format!("--{} is required", name))),
    }
}

fn glyph_pdf(req: &Request) -> Result<Outcome> {
    let input = PathBuf::from(req.position(0, "input")?);
    let out = PathBuf::from(required_value(req, "out")?);
    let title = req.value("title").unwrap_or("Untitled");

    let text = fs::read_to_string(&input)
        .map_err(|e| Error::Invalid(format!("{}: {}", input.display(), e)))?;
    let lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();

    let report = savehorse_glyph::create_pdf(title, &lines, &out)?;

    Ok(Outcome::new(
        format!(
            "wrote '{}' — {} byte(s), {} line(s), title \"{}\"\n",
            report.file_path, report.bytes_written, report.lines, report.title
        ),
        to_json(&report)?,
    ))
}

fn glyph_extract(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let report = savehorse_glyph::extract_pdf_text(&path)?;

    let mut text = report.extracted_text.clone();
    text.push_str(&format!(
        "\n-- {} page(s), {} line(s) extracted\n",
        report.total_pages, report.lines_count
    ));

    Ok(Outcome::new(text, to_json(&report)?))
}

fn glyph_markdown(req: &Request) -> Result<Outcome> {
    let input = PathBuf::from(req.position(0, "input")?);
    let title = req.value("title").unwrap_or("Document");

    let md = fs::read_to_string(&input)
        .map_err(|e| Error::Invalid(format!("{}: {}", input.display(), e)))?;
    let html = savehorse_glyph::compile_markdown_to_html(&md, title);

    match req.value("out") {
        Some(dest) if !dest.trim().is_empty() => {
            fs::write(dest, &html)?;
            Ok(Outcome::new(
                format!("wrote '{}' — {} byte(s)\n", dest, html.len()),
                serde_json::json!({ "output_file": dest, "bytes": html.len() }).to_string(),
            ))
        }
        _ => Ok(Outcome::new(
            format!("{}\n", html),
            serde_json::json!({ "html": html }).to_string(),
        )),
    }
}

fn glyph_encoding(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let r = savehorse_glyph::inspect_encoding(&path)?;

    let text = format!(
        "file       {}\nencoding   {}\nbom        {}\nendings    {} (CRLF {}, LF {})\n",
        r.file_path, r.detected_encoding, r.has_bom, r.line_terminator, r.crlf_count, r.lf_count
    );
    Ok(Outcome::new(text, to_json(&r)?))
}

fn glyph_readability(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let r = savehorse_glyph::analyze_file_readability(&path)?;

    let text = format!(
        "words      {}\ncharacters {}\nsentences  {}\navg word   {}\nreading    ~{} min at 200 wpm\n",
        r.total_words,
        r.total_characters,
        r.total_sentences,
        r.average_word_length,
        r.estimated_reading_time_mins
    );
    Ok(Outcome::new(text, to_json(&r)?))
}

fn glyph_metrics(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let r = savehorse_glyph::analyze_code_metrics(&path)?;

    let text = format!(
        "file       {}\ntotal      {}\ncode       {}\ncomments   {} ({}%)\nblank      {}\ncharacters {}\n",
        r.file_path,
        r.total_lines,
        r.code_lines,
        r.comment_lines,
        r.comment_ratio_percent,
        r.blank_lines,
        r.character_count
    );
    Ok(Outcome::new(text, to_json(&r)?))
}

// ---------------------------------------------------------------------------
// halo
// ---------------------------------------------------------------------------

fn halo_create(req: &Request) -> Result<Outcome> {
    let source = PathBuf::from(req.position(0, "source")?);
    let out = PathBuf::from(required_value(req, "out")?);
    let volume_id = req.value("volume-id").unwrap_or("SAVEHORSE");

    let report = savehorse_halo::create_image(&source, &out, volume_id)?;

    let mut text = format!(
        "created '{}' — {} file(s), {} sector(s), {} byte(s), volume '{}'\n",
        report.image_path,
        report.files_count,
        report.total_sectors,
        report.bytes_written,
        report.volume_id
    );
    for e in &report.entries {
        text.push_str(&format!(
            "  {:<28} sector {:<8} {} byte(s)\n",
            e.name, e.extent_sector, e.size_bytes
        ));
    }

    Ok(Outcome::new(text, to_json(&report)?))
}

fn halo_list(req: &Request) -> Result<Outcome> {
    let image = PathBuf::from(req.position(0, "image")?);
    let info = savehorse_halo::read_metadata(&image)?;

    let mut text = format!(
        "image      {}\nvolume     {}\nsectors    {} ({} KB)\n\n{:<28} {:<10} {}\n{}\n",
        image.display(),
        info.volume_id,
        info.total_sectors,
        info.total_sectors * 2,
        "NAME",
        "SECTOR",
        "BYTES",
        "-".repeat(56)
    );
    for e in &info.entries {
        text.push_str(&format!(
            "{:<28} {:<10} {}\n",
            e.name, e.extent_sector, e.size_bytes
        ));
    }
    text.push_str(&format!("{}\n{} file(s)\n", "-".repeat(56), info.entries.len()));

    Ok(Outcome::new(text, to_json(&info)?))
}

fn halo_extract(req: &Request) -> Result<Outcome> {
    let image = PathBuf::from(req.position(0, "image")?);
    let out = PathBuf::from(required_value(req, "out")?);

    let report = savehorse_halo::extract_image(&image, &out, req.value("filter"))?;

    let mut text = format!(
        "extracted {} file(s) from '{}' to '{}'\n",
        report.extracted.len(),
        report.image_path,
        report.output_dir
    );
    for e in &report.extracted {
        text.push_str(&format!("  {}\n", e.name));
    }

    Ok(Outcome::new(text, to_json(&report)?))
}

fn halo_verify(req: &Request) -> Result<Outcome> {
    let image = PathBuf::from(req.position(0, "image")?);
    let r = savehorse_halo::verify_image(&image)?;

    let mut text = if r.valid {
        format!("OK '{}' is a valid ISO 9660 image\n", image.display())
    } else {
        format!("FAILED '{}' has problems\n", image.display())
    };
    text.push_str(&format!(
        "volume     {}\nsectors    {}\nfiles      {}\n{}\n",
        r.volume_id, r.total_sectors, r.files_count, r.message
    ));
    for issue in &r.issues {
        text.push_str(&format!("  ! {}\n", issue));
    }

    Ok(Outcome::new(text, to_json(&r)?))
}

fn halo_convert(req: &Request) -> Result<Outcome> {
    let source = PathBuf::from(req.position(0, "source")?);
    let out = PathBuf::from(required_value(req, "out")?);

    let r = savehorse_halo::convert_raw_to_iso(&source, &out)?;

    Ok(Outcome::new(
        format!(
            "converted '{}' -> '{}' ({} raw sector(s) -> {} byte(s))\n",
            r.source_file, r.output_file, r.source_sectors, r.output_bytes
        ),
        to_json(&r)?,
    ))
}


// ---------------------------------------------------------------------------
// iris
// ---------------------------------------------------------------------------

fn iris_info(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let i = savehorse_iris::inspect_image(&path)?;

    let text = format!(
        "file       {}\nformat     {}\nsize       {}x{}\naspect     {} ({})\ncolour     {}\nbytes      {}\n",
        i.file, i.format, i.width, i.height, i.aspect_ratio, i.aspect_ratio_float, i.color_type, i.byte_size
    );
    Ok(Outcome::new(text, to_json(&i)?))
}

fn iris_convert(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let out = PathBuf::from(required_value(req, "out")?);
    let r = savehorse_iris::convert_image(&path, &out)?;

    Ok(Outcome::new(
        format!(
            "converted '{}' -> '{}' ({}x{}, {} byte(s))\n",
            r.source_file, r.output_file, r.width, r.height, r.output_byte_size
        ),
        to_json(&r)?,
    ))
}

fn iris_thumb(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let out = PathBuf::from(required_value(req, "out")?);
    let width = req.number("width")?.ok_or_else(|| Error::Invalid("--width is required".to_string()))? as u32;
    let height = req.number("height")?.ok_or_else(|| Error::Invalid("--height is required".to_string()))? as u32;

    let r = savehorse_iris::thumb_image(&path, &out, width, height)?;

    Ok(Outcome::new(
        format!(
            "thumbnail '{}' -> '{}' ({} -> {})\n",
            r.source_file, r.output_file, r.original_dimensions, r.thumbnail_dimensions
        ),
        to_json(&r)?,
    ))
}

fn iris_transform(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let out = PathBuf::from(required_value(req, "out")?);
    let rotate = req.number("rotate")?.map(|n| n as u32);

    let r = savehorse_iris::transform_image(&path, &out, rotate, req.has("flip-h"), req.has("flip-v"))?;

    let mut what = Vec::new();
    if let Some(d) = r.rotation {
        what.push(format!("rotate {}", d));
    }
    if r.flip_horizontal {
        what.push("flip horizontal".to_string());
    }
    if r.flip_vertical {
        what.push("flip vertical".to_string());
    }
    if what.is_empty() {
        what.push("no change".to_string());
    }

    Ok(Outcome::new(
        format!(
            "'{}' -> '{}' ({}) now {}\n",
            r.source_file, r.output_file, what.join(", "), r.new_dimensions
        ),
        to_json(&r)?,
    ))
}

fn iris_stats(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let s = savehorse_iris::stats_image(&path)?;

    let text = format!(
        "file       {}\nsize       {}x{} ({} pixels)\nred        {}\ngreen      {}\nblue       {}\nalpha      {}\nluminance  {}\n",
        s.file, s.width, s.height, s.total_pixels, s.mean_red, s.mean_green, s.mean_blue, s.mean_alpha, s.mean_luminance
    );
    Ok(Outcome::new(text, to_json(&s)?))
}

fn iris_batch(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let out = PathBuf::from(required_value(req, "out")?);
    let format = required_value(req, "format")?;
    let size = req.number("size")?.map(|n| n as u32);

    let r = savehorse_iris::batch_process(&path, &out, format, size)?;

    let mut text = format!(
        "{} image(s) -> {} ({} failed)\n",
        r.processed.len(),
        r.target_format,
        r.failed.len()
    );
    for p in &r.processed {
        text.push_str(&format!("  {}\n", p));
    }
    for f in &r.failed {
        text.push_str(&format!("  ! {}\n", f));
    }

    Ok(Outcome::new(text, to_json(&r)?))
}

// ---------------------------------------------------------------------------
// arc
// ---------------------------------------------------------------------------

fn arc_create(req: &Request) -> Result<Outcome> {
    let out = PathBuf::from(required_value(req, "out")?);
    let paths: Vec<PathBuf> = req.positional.iter().map(PathBuf::from).collect();
    if paths.is_empty() {
        return Err(Error::Invalid("at least one path to archive is required".to_string()));
    }

    let r = savehorse_arc::create_archive(&out, &paths, req.has("store"))?;

    Ok(Outcome::new(
        format!(
            "created '{}' — {} file(s), {} -> {} byte(s), {}\n",
            r.archive, r.files_added, r.bytes_in, r.bytes_out, r.compression_method
        ),
        to_json(&r)?,
    ))
}

fn arc_extract(req: &Request) -> Result<Outcome> {
    let archive = PathBuf::from(req.position(0, "archive")?);
    let out = PathBuf::from(required_value(req, "out")?);

    let r = savehorse_arc::extract_archive(&archive, &out, req.value("filter"))?;

    let mut text = format!(
        "extracted {} file(s) from '{}' to '{}'\n",
        r.extracted.len(),
        r.archive,
        r.output_dir
    );
    if r.skipped > 0 {
        text.push_str(&format!("  {} entr(ies) skipped as unsafe or unreadable\n", r.skipped));
    }

    Ok(Outcome::new(text, to_json(&r)?))
}

fn arc_list(req: &Request) -> Result<Outcome> {
    let archive = PathBuf::from(req.position(0, "archive")?);
    let entries = savehorse_arc::list_archive(&archive)?;

    let total_uncompressed: u64 = entries.iter().map(|e| e.uncompressed_size).sum();
    let total_compressed: u64 = entries.iter().map(|e| e.compressed_size).sum();

    let mut text = format!(
        "{:<10} {:<10} {:<8} {:<10} {}\n{}\n",
        "LENGTH", "COMPRESSED", "RATIO", "CRC32", "NAME", "-".repeat(65)
    );
    for e in &entries {
        text.push_str(&format!(
            "{:<10} {:<10} {:<8} {:<10} {}\n",
            e.uncompressed_size, e.compressed_size, e.compression_ratio, e.crc32, e.name
        ));
    }
    text.push_str(&format!(
        "{}\n{:<10} {:<10}          {} item(s)\n",
        "-".repeat(65),
        total_uncompressed,
        total_compressed,
        entries.len()
    ));

    Ok(Outcome::new(text, to_json(&entries)?))
}

fn arc_test(req: &Request) -> Result<Outcome> {
    let archive = PathBuf::from(req.position(0, "archive")?);
    let r = savehorse_arc::test_archive(&archive)?;

    let mut text = if r.failed.is_empty() {
        format!("OK '{}' — all {} entr(ies) read cleanly\n", r.archive, r.passed)
    } else {
        format!("FAILED '{}' — {} of {} entr(ies) failed\n", r.archive, r.failed.len(), r.entries)
    };
    for f in &r.failed {
        text.push_str(&format!("  ! {}\n", f));
    }

    Ok(Outcome::new(text, to_json(&r)?))
}

fn arc_split(req: &Request) -> Result<Outcome> {
    let file = PathBuf::from(req.position(0, "path")?);
    let out = PathBuf::from(required_value(req, "out")?);
    let size = req
        .size("size")?
        .ok_or_else(|| Error::Invalid("--size is required, e.g. 10m".to_string()))?;

    let manifest = savehorse_arc::split_file(&file, size, &out)?;

    Ok(Outcome::new(
        format!(
            "split '{}' ({} byte(s)) into {} chunk(s) of {} byte(s) in '{}'\n",
            manifest.original_filename,
            manifest.total_size,
            manifest.chunk_count,
            manifest.chunk_size,
            out.display()
        ),
        to_json(&manifest)?,
    ))
}

fn arc_join(req: &Request) -> Result<Outcome> {
    let input = PathBuf::from(req.position(0, "input")?);
    let out = req.value("out").map(PathBuf::from);

    let r = savehorse_arc::join_chunks(&input, out.as_deref())?;

    Ok(Outcome::new(
        format!(
            "joined {} chunk(s) into '{}' ({} byte(s))\n",
            r.chunks, r.output, r.total_bytes
        ),
        to_json(&r)?,
    ))
}


// ---------------------------------------------------------------------------
// revive
// ---------------------------------------------------------------------------

fn revive_bounds(req: &Request) -> Result<(Option<usize>, Option<usize>)> {
    let min = req.size("min-size")?.map(|n| n as usize);
    let max = req.size("max-size")?.map(|n| n as usize);
    Ok((min, max))
}

fn revive_scan(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let (min, max) = revive_bounds(req)?;
    let summary = savehorse_revive::scan_file(&path, req.value("types"), min, max)?;

    let mut text = format!(
        "scanned {} byte(s) of '{}'\nfound {} artifact(s), {} byte(s) recoverable\n\n",
        summary.total_scanned_bytes, summary.source_file, summary.artifacts_found, summary.total_recovered_bytes
    );
    for a in &summary.artifacts {
        text.push_str(&format!(
            "  #{:<4} {:<16} 0x{:08X}  {:>9} byte(s)  {}\n",
            a.index, a.format, a.offset_bytes, a.length_bytes, &a.sha256[..16.min(a.sha256.len())]
        ));
    }

    Ok(Outcome::new(text, to_json(&summary)?))
}

fn revive_carve(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let out = PathBuf::from(required_value(req, "out")?);
    let (min, max) = revive_bounds(req)?;

    let summary = savehorse_revive::carve_file(&path, &out, req.value("types"), min, max)?;

    let mut text = format!(
        "carved {} file(s) from '{}' into '{}' ({} byte(s))\n",
        summary.artifacts_found, summary.source_file, out.display(), summary.total_recovered_bytes
    );
    for a in &summary.artifacts {
        if let Some(ref saved) = a.saved_filename {
            text.push_str(&format!("  {}\n", saved));
        }
    }

    Ok(Outcome::new(text, to_json(&summary)?))
}

fn revive_report(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let summary = savehorse_revive::scan_file(&path, None, None, None)?;
    let report = savehorse_revive::build_report(&summary);

    match req.value("out") {
        Some(dest) if !dest.trim().is_empty() => {
            fs::write(dest, &report)?;
            Ok(Outcome::new(
                format!("wrote a recovery report for '{}' to '{}'\n", path.display(), dest),
                to_json(&summary)?,
            ))
        }
        _ => Ok(Outcome::new(report, to_json(&summary)?)),
    }
}

fn revive_pack(req: &Request) -> Result<Outcome> {
    let source = PathBuf::from(req.position(0, "source")?);
    let out = PathBuf::from(required_value(req, "out")?);
    let r = savehorse_revive::pack_directory(&source, &out)?;

    Ok(Outcome::new(
        format!(
            "packed {} file(s), {} byte(s) into '{}'\n",
            r.files_archived, r.total_bytes_archived, r.archive_path
        ),
        to_json(&r)?,
    ))
}

fn revive_unpack(req: &Request) -> Result<Outcome> {
    let archive = PathBuf::from(req.position(0, "archive")?);
    let out = PathBuf::from(required_value(req, "out")?);
    let r = savehorse_revive::unpack_archive(&archive, &out)?;

    Ok(Outcome::new(
        format!(
            "restored {} file(s), {} byte(s) to '{}'\n",
            r.files_extracted, r.total_bytes_extracted, r.destination_directory
        ),
        to_json(&r)?,
    ))
}

fn revive_verify(req: &Request) -> Result<Outcome> {
    let archive = PathBuf::from(req.position(0, "archive")?);
    let r = savehorse_revive::verify_archive(&archive)?;

    let mut text = if r.all_passed {
        format!("OK '{}' — all {} file(s) match their checksums\n", r.archive_path, r.valid_files)
    } else {
        format!(
            "FAILED '{}' — {} of {} file(s) corrupted\n",
            r.archive_path, r.corrupted_files, r.total_files
        )
    };
    for entry in r.results.iter().filter(|e| !e.is_valid) {
        text.push_str(&format!(
            "  ! {} (expected {:08x}, found {:08x})\n",
            entry.path, entry.expected_crc32, entry.actual_crc32
        ));
    }

    Ok(Outcome::new(text, to_json(&r)?))
}

fn revive_geometry(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let r = savehorse_revive::scan_disk_image(&path)?;

    let text = format!(
        "file       {}\nbytes      {}\nsectors    {} (512 byte)\npartition  {}\nzero       {}\ndata       {}\nentropy    {} bits/byte\n",
        r.target_file, r.total_bytes, r.total_sectors, r.partition_scheme, r.zero_sectors, r.data_sectors, r.average_entropy
    );
    Ok(Outcome::new(text, to_json(&r)?))
}

// ---------------------------------------------------------------------------
// sift — the text half
// ---------------------------------------------------------------------------

fn sift_view(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let start = req.number("start")?.unwrap_or(1);
    let r = savehorse_sift::view(&path, req.has("numbers"), start, req.number("count")?)?;

    Ok(Outcome::new(r.text.clone(), to_json(&r)?))
}

fn sift_eol(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let r = savehorse_sift::normalize_eol(&path, req.value("to"), req.has("in-place"))?;

    let mut text = format!(
        "file       {}\nCRLF       {}\nLF         {}\nCR         {}\ndominant   {}\n",
        r.file, r.crlf_count, r.lf_count, r.cr_count, r.detected_dominant
    );
    match r.converted_to {
        Some(ref style) if r.in_place => text.push_str(&format!("rewritten  {}\n", style)),
        Some(ref style) => text.push_str(&format!("would write {} — add --in-place to apply\n", style)),
        None => text.push_str("no conversion asked for\n"),
    }

    let mut outcome = Outcome::new(text, to_json(&r)?);
    if r.converted_to.is_some() && !r.in_place {
        outcome = outcome.preview();
    }
    Ok(outcome)
}

fn sift_clean(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let r = savehorse_sift::clean_text(
        &path,
        req.has("strip-trailing"),
        req.number("tab-size")?,
        req.has("collapse-blank"),
        req.has("in-place"),
    )?;

    let mut text = format!(
        "file       {}\nlines      {}\ntrailing   {}\ntabs       {}\nblanks     {}\n",
        r.file, r.lines_processed, r.trailing_spaces_removed, r.tabs_expanded, r.blank_lines_collapsed
    );
    if r.in_place {
        text.push_str("rewritten\n");
    } else {
        text.push_str("nothing written — add --in-place to apply\n");
    }

    let mut outcome = Outcome::new(text, to_json(&r)?);
    if !r.in_place {
        outcome = outcome.preview();
    }
    Ok(outcome)
}

fn sift_stats(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let s = savehorse_sift::document_stats(&path)?;

    let text = format!(
        "file       {}\nbytes      {}\ncharacters {}\nwords      {}\nunique     {}\nlines      {}\nempty      {}\navg word   {}\nlongest    {}\n",
        s.file, s.total_bytes, s.total_characters, s.total_words, s.unique_words,
        s.total_lines, s.empty_lines, s.average_word_length, s.longest_line_chars
    );
    Ok(Outcome::new(text, to_json(&s)?))
}

fn sift_convert(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let to = required_value(req, "to")?;
    let out = PathBuf::from(required_value(req, "out")?);

    let r = savehorse_sift::convert_encoding(&path, to, &out)?;

    Ok(Outcome::new(
        format!(
            "converted '{}' ({}) -> '{}' as {}\n",
            r.file,
            r.detected_encoding,
            r.output_path.clone().unwrap_or_default(),
            r.converted_to.clone().unwrap_or_default()
        ),
        to_json(&r)?,
    ))
}


// ---------------------------------------------------------------------------
// forge
// ---------------------------------------------------------------------------

fn forge_inspect(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let i = savehorse_forge::inspect_pe(&path)?;

    let text = format!(
        "file       {}\nmachine    {}\n64-bit     {}\nsections   {}\nentry rva  0x{:X}\nimage base 0x{:X}\nsubsystem  {} ({})\nbytes      {}\n",
        i.file_path, i.machine_type, i.is_64bit, i.number_of_sections,
        i.entry_point_rva, i.image_base, i.subsystem_name, i.subsystem_id, i.file_size_bytes
    );
    Ok(Outcome::new(text, to_json(&i)?))
}

fn forge_sections(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let sections = savehorse_forge::inspect_sections(&path)?;

    let mut text = format!(
        "{:<10} {:<10} {:<10} {:<12} {:<9} {}\n{}\n",
        "NAME", "VIRT ADDR", "VIRT SIZE", "RAW SIZE", "ENTROPY", "FLAGS", "-".repeat(64)
    );
    for s in &sections {
        text.push_str(&format!(
            "{:<10} 0x{:<8X} {:<10} {:<12} {:<9.4} {}{}\n",
            s.name, s.virtual_address, s.virtual_size, s.raw_data_size, s.entropy,
            s.characteristics, if s.is_packed { "  PACKED?" } else { "" }
        ));
    }
    text.push_str(&format!("\n{} section(s)\n", sections.len()));

    Ok(Outcome::new(text, to_json(&sections)?))
}

fn forge_imports(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let dlls = savehorse_forge::extract_imports(&path)?;

    let mut text = format!("{} library name(s) found by scanning for .dll strings\n", dlls.len());
    for d in &dlls {
        text.push_str(&format!("  {}\n", d));
    }

    Ok(Outcome::new(text, to_json(&dlls)?))
}

fn forge_hexdump(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let offset = req.number("offset")?.unwrap_or(0) as u64;
    let length = req.number("length")?.unwrap_or(256);

    let lines = savehorse_forge::hexdump_file(&path, offset, length)?;

    let mut text = String::new();
    for line in &lines {
        text.push_str(&format!("{:08X}  {}  |{}|\n", line.offset, line.hex_bytes, line.ascii));
    }
    text.push_str(&format!("{} line(s)\n", lines.len()));

    Ok(Outcome::new(text, to_json(&lines)?))
}

fn forge_search(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let hex = required_value(req, "hex")?;
    let hits = savehorse_forge::search_hex_pattern(&path, hex)?;

    let mut text = format!("{} match(es)\n", hits.len());
    for hit in &hits {
        text.push_str(&format!("  0x{:08X}\n", hit));
    }

    Ok(Outcome::new(text, to_json(&hits)?))
}

fn forge_patch(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let offset = req.number("offset")?.unwrap_or(0) as u64;
    let hex = required_value(req, "hex")?;
    let write = !req.has("dry-run");

    let r = savehorse_forge::apply_patch(&path, offset, hex, req.has("backup"), write)?;

    let text = if r.written {
        format!(
            "patched {} byte(s) at 0x{:X} in '{}': {} -> {}{}\n",
            r.bytes_patched, r.offset, r.file, r.original_bytes, r.new_bytes,
            r.backup_created.clone().map(|b| format!(" (backup {})", b)).unwrap_or_default()
        )
    } else {
        format!(
            "would patch {} byte(s) at 0x{:X} in '{}': {} -> {}\n(re-run without --dry-run to apply)\n",
            r.bytes_patched, r.offset, r.file, r.original_bytes, r.new_bytes
        )
    };

    let mut outcome = Outcome::new(text, to_json(&r)?);
    if !r.written {
        outcome = outcome.preview();
    }
    Ok(outcome)
}

// ---------------------------------------------------------------------------
// morph
// ---------------------------------------------------------------------------

fn morph_rename(req: &Request) -> Result<Outcome> {
    let paths: Vec<PathBuf> = req.positional.iter().map(PathBuf::from).collect();
    if paths.is_empty() {
        return Err(Error::Invalid("at least one path is required".to_string()));
    }

    let mut options = savehorse_morph::options_with_defaults();
    options.pattern = req.value("pattern").map(|s| s.to_string());
    options.replace = req.value("replace").unwrap_or("").to_string();
    options.prefix = req.value("prefix").map(|s| s.to_string());
    options.suffix = req.value("suffix").map(|s| s.to_string());
    options.case = match req.value("case") {
        Some(c) if !c.trim().is_empty() => Some(savehorse_morph::CaseMode::parse(c)?),
        _ => None,
    };
    options.extension = req.value("ext").map(|s| s.to_string());
    options.lowercase_extension = req.has("ext-lower");
    options.number = req.has("number");
    if let Some(start) = req.number("start")? {
        options.start = start;
    }
    if let Some(step) = req.number("step")? {
        options.step = step;
    }
    if let Some(pad) = req.number("pad")? {
        options.pad = pad;
    }

    let dry_run = req.has("dry-run");

    // A rename you cannot undo is worse than one you did not do, so a real run
    // always leaves a journal — a default one if none was named.
    let journal: Option<PathBuf> = match req.value("journal") {
        Some(j) if !j.trim().is_empty() => Some(PathBuf::from(j)),
        _ if dry_run => None,
        _ => {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            Some(PathBuf::from(format!(".savehorse-rename-{}.json", stamp)))
        }
    };

    let report = savehorse_morph::rename(&paths, &options, dry_run, journal.as_deref())?;

    let mut text = if report.dry_run {
        format!("preview: {} file(s) considered, {} would change\n", report.considered, report.changed)
    } else {
        format!(
            "renamed {} of {} file(s)\n",
            report.changed, report.considered
        )
    };
    for item in report.items.iter().filter(|i| i.changed) {
        text.push_str(&format!("  {} -> {}\n", item.original_name, item.new_name));
    }
    if let Some(ref j) = report.journal_path {
        text.push_str(&format!("undo journal: {}\n", j));
    }
    if report.dry_run {
        text.push_str("nothing renamed — re-run without --dry-run to apply\n");
    }

    let mut outcome = Outcome::new(text, to_json(&report)?);
    if report.dry_run {
        outcome = outcome.preview();
    }
    Ok(outcome)
}

fn morph_undo(req: &Request) -> Result<Outcome> {
    let journal = PathBuf::from(req.position(0, "journal")?);
    let dry_run = req.has("dry-run");

    let report = savehorse_morph::undo(&journal, dry_run)?;

    let mut text = if report.dry_run {
        format!("preview: {} rename(s) would be reversed\n", report.changed)
    } else {
        format!("reversed {} rename(s)\n", report.changed)
    };
    for item in &report.items {
        text.push_str(&format!("  {} -> {}\n", item.original_name, item.new_name));
    }

    let mut outcome = Outcome::new(text, to_json(&report)?);
    if report.dry_run {
        outcome = outcome.preview();
    }
    Ok(outcome)
}


// ---------------------------------------------------------------------------
// aegis
// ---------------------------------------------------------------------------

fn aegis_encrypt(req: &Request) -> Result<Outcome> {
    let input = PathBuf::from(req.position(0, "path")?);
    let out = req.value("out").map(PathBuf::from);
    let keyfile = req.value("keyfile").map(PathBuf::from);

    let r = savehorse_aegis::encrypt(&input, out.as_deref(), req.value("passphrase"), keyfile.as_deref())?;

    Ok(Outcome::new(
        format!(
            "encrypted '{}' -> '{}' ({} -> {} byte(s), {})\n",
            r.input_file, r.output_file, r.input_size_bytes, r.output_size_bytes, r.cipher
        ),
        to_json(&r)?,
    ))
}

fn aegis_decrypt(req: &Request) -> Result<Outcome> {
    let input = PathBuf::from(req.position(0, "path")?);
    let out = req.value("out").map(PathBuf::from);
    let keyfile = req.value("keyfile").map(PathBuf::from);

    let r = savehorse_aegis::decrypt(&input, out.as_deref(), req.value("passphrase"), keyfile.as_deref())?;

    Ok(Outcome::new(
        format!(
            "decrypted '{}' -> '{}' ({} byte(s) recovered)\n",
            r.input_file, r.output_file, r.output_size_bytes
        ),
        to_json(&r)?,
    ))
}

fn aegis_shred(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let dry_run = req.has("dry-run");

    let r = savehorse_aegis::shred(&path, dry_run)?;

    let text = if r.dry_run {
        format!(
            "would overwrite '{}' three times ({} byte(s)), truncate it and unlink it\n(re-run without --dry-run to destroy the file)\n",
            r.original_path, r.original_size_bytes
        )
    } else {
        format!(
            "shredded '{}' — {} byte(s), {} passes, unlinked\n",
            r.original_path, r.original_size_bytes, r.passes_completed
        )
    };

    let mut outcome = Outcome::new(text, to_json(&r)?);
    if r.dry_run {
        outcome = outcome.preview();
    }
    Ok(outcome)
}

fn aegis_scan(req: &Request) -> Result<Outcome> {
    let target = PathBuf::from(req.position(0, "path")?);

    let signatures = match req.value("signatures") {
        Some(path) if !path.trim().is_empty() => {
            savehorse_aegis::load_signatures(&PathBuf::from(path))?
        }
        _ => Vec::new(),
    };

    let r = savehorse_aegis::scan(&target, &signatures)?;

    let mut text = format!(
        "scanned {} file(s) in '{}'\n{} threat(s) found; {} signature(s) loaded\n",
        r.files_scanned, r.target_path, r.threats_found, r.signatures_loaded
    );
    for threat in &r.threats {
        text.push_str(&format!(
            "  [{}] {} — {}\n    {}\n",
            threat.severity, threat.threat_name, threat.file_path, threat.detail
        ));
    }
    if r.clean {
        text.push_str("clean\n");
    }

    Ok(Outcome::new(text, to_json(&r)?))
}

fn aegis_harden(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let f = savehorse_aegis::harden(&path)?;

    let mark = |on: bool| if on { "yes" } else { "NO" };
    let text = format!(
        "file       {}\nmachine    {} ({}-bit)\nDEP        {}\nASLR       {}\nhigh-entropy ASLR {}\nCFG        {}\nsigned     {} (certificate table present)\ndll chars  0x{:04X}\n",
        f.file_path,
        f.machine_type,
        if f.is_64bit { 64 } else { 32 },
        mark(f.dep),
        mark(f.aslr),
        mark(f.high_entropy_aslr),
        mark(f.control_flow_guard),
        if f.signed_heuristic { "yes" } else { "no" },
        f.dll_characteristics
    );

    Ok(Outcome::new(text, to_json(&f)?))
}

// ---------------------------------------------------------------------------
// pulse
// ---------------------------------------------------------------------------

fn pulse_hash(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let h = savehorse_pulse::hash_file(&path)?;

    let text = format!(
        "file       {}\nsize       {} ({})\ncrc32      {}\nsha256     {}\n",
        h.file, h.size_bytes, h.size_human, h.crc32, h.sha256
    );
    Ok(Outcome::new(text, to_json(&h)?))
}

fn pulse_temp(req: &Request) -> Result<Outcome> {
    let directory = req.positional.first().map(PathBuf::from);
    let top = req.number("top")?.unwrap_or(10);

    let r = savehorse_pulse::audit_temp(directory.as_deref(), top)?;

    let mut text = format!(
        "directory  {}\nfiles      {}\ntotal      {} ({} byte(s))\n\nLARGEST\n",
        r.directory, r.files, r.total_human, r.total_bytes
    );
    for entry in &r.largest {
        text.push_str(&format!("  {:>10}  {}\n", entry.size_human, entry.path));
    }
    text.push_str("\nnothing was deleted\n");

    Ok(Outcome::new(text, to_json(&r)?))
}


// ---------------------------------------------------------------------------
// the tails: halo, iris, arc and scout extras
// ---------------------------------------------------------------------------

fn halo_cue(req: &Request) -> Result<Outcome> {
    let image = PathBuf::from(req.position(0, "image")?);
    let out = PathBuf::from(required_value(req, "out")?);
    let mode = req.value("mode").unwrap_or("");

    let r = savehorse_halo::write_cue_sheet(&image, &out, mode)?;

    Ok(Outcome::new(
        format!("wrote '{}'\n\n{}", r.cue_path, r.content),
        to_json(&r)?,
    ))
}

fn halo_cover(req: &Request) -> Result<Outcome> {
    let image = PathBuf::from(req.position(0, "image")?);
    let out = PathBuf::from(required_value(req, "out")?);

    let r = savehorse_halo::write_cover_art(&image, &out)?;

    Ok(Outcome::new(
        format!(
            "wrote '{}' — volume '{}', {} file(s) listed, {} byte(s)\n",
            r.svg_path, r.volume_id, r.files_listed, r.bytes
        ),
        to_json(&r)?,
    ))
}

fn iris_ico(req: &Request) -> Result<Outcome> {
    let source = PathBuf::from(req.position(0, "path")?);
    let out = PathBuf::from(required_value(req, "out")?);

    let sizes: Vec<u32> = match req.value("sizes") {
        Some(list) if !list.trim().is_empty() => list
            .split(',')
            .filter_map(|s| s.trim().parse::<u32>().ok())
            .collect(),
        _ => vec![16, 32, 48, 256],
    };

    let r = savehorse_iris::write_ico(&source, &out, &sizes)?;

    Ok(Outcome::new(
        format!(
            "wrote '{}' with {} size(s): {} — {} byte(s)\n",
            r.output_file,
            r.sizes.len(),
            r.sizes.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(", "),
            r.output_byte_size
        ),
        to_json(&r)?,
    ))
}

fn iris_sheet(req: &Request) -> Result<Outcome> {
    let source = PathBuf::from(req.position(0, "path")?);
    let out = PathBuf::from(required_value(req, "out")?);
    let cell = req.number("cell")?.unwrap_or(128) as u32;
    let columns = req.number("columns")?.unwrap_or(4);

    let r = savehorse_iris::contact_sheet_svg(&source, &out, cell, columns)?;

    Ok(Outcome::new(
        format!(
            "wrote '{}' — {} image(s) in {} column(s), {} byte(s)\n",
            r.output_file, r.images, r.columns, r.output_byte_size
        ),
        to_json(&r)?,
    ))
}

fn arc_chunk(req: &Request) -> Result<Outcome> {
    let total = req
        .size("total")?
        .ok_or_else(|| Error::Invalid("--total is required, e.g. 100m".to_string()))?;
    let segments = req.number("segments")?.unwrap_or(8);

    let plans = savehorse_arc::plan_segments(total, segments);

    let mut text = format!("{} range(s) for {} byte(s)\n", plans.len(), total);
    for plan in &plans {
        text.push_str(&format!(
            "  #{:<3} {:<22} {} byte(s)\n",
            plan.segment_index, plan.range_header, plan.length_bytes
        ));
    }

    Ok(Outcome::new(text, to_json(&plans)?))
}

fn arc_torrent(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let m = savehorse_arc::parse_torrent(&path)?;

    let mut text = format!(
        "name       {}\ntotal      {} byte(s) in {} file(s)\npieces     {} x {} byte(s)\ntracker    {}\n",
        m.name,
        m.total_bytes,
        m.files.len(),
        m.piece_count,
        m.piece_length,
        m.announce.clone().unwrap_or_else(|| "none".to_string())
    );
    for file in m.files.iter().take(20) {
        text.push_str(&format!("  {:>12}  {}\n", file.length, file.path));
    }

    Ok(Outcome::new(text, to_json(&m)?))
}

fn arc_probe(req: &Request) -> Result<Outcome> {
    let host = required_value(req, "host")?;
    let port = req
        .number("port")?
        .ok_or_else(|| Error::Invalid("--port is required".to_string()))? as u16;
    let timeout = req.number("timeout")?.unwrap_or(2000) as u64;

    let r = savehorse_arc::probe_tcp(host, port, timeout)?;

    let text = format!(
        "{}:{}\n{}\n{} ms\n{}\n",
        r.host,
        r.port,
        if r.connected { "connected" } else { "not connected" },
        r.latency_ms,
        r.detail
    );

    Ok(Outcome::new(text, to_json(&r)?))
}

fn arc_upnp(req: &Request) -> Result<Outcome> {
    let external = req
        .number("external-port")?
        .ok_or_else(|| Error::Invalid("--external-port is required".to_string()))? as u16;
    let internal = req
        .number("internal-port")?
        .ok_or_else(|| Error::Invalid("--internal-port is required".to_string()))? as u16;
    let ip = required_value(req, "ip")?;
    let protocol = req.value("protocol").unwrap_or("TCP");

    let payloads = savehorse_arc::upnp_payloads(external, internal, ip, protocol);

    let text = format!(
        "SSDP DISCOVERY\n{}\nSOAP ADD PORT MAPPING\n{}\n(nothing was sent: these are the payloads only)\n",
        payloads.ssdp_discovery, payloads.soap_add_port_mapping
    );

    Ok(Outcome::new(text, to_json(&payloads)?))
}

fn scout_treemap(req: &Request) -> Result<Outcome> {
    let root = PathBuf::from(req.position(0, "path")?);
    let report = savehorse_scout::analyze_treemap(&root)?;

    let mut text = format!(
        "{} — {} in {} item(s)\n",
        report.root_path,
        savehorse_core::format_bytes(report.total_size_bytes),
        report.total_items
    );
    for item in report.items.iter().take(15) {
        text.push_str(&format!(
            "  {:>6.2}%  {:>10}  {}{}\n",
            item.percentage,
            savehorse_core::format_bytes(item.size_bytes),
            item.name,
            if item.is_dir { "/" } else { "" }
        ));
    }

    match req.value("out") {
        Some(dest) if !dest.trim().is_empty() => {
            let width = req.number("width")?.unwrap_or(800) as u32;
            let svg = savehorse_scout::render_treemap_svg(&report, width);
            fs::write(dest, &svg)?;
            text.push_str(&format!("\nwrote '{}'\n", dest));
        }
        _ => {}
    }

    Ok(Outcome::new(text, to_json(&report)?))
}

fn scout_inventory(req: &Request) -> Result<Outcome> {
    let root = PathBuf::from(req.position(0, "path")?);
    let r = savehorse_scout::inspect_directory(&root)?;

    let text = format!(
        "directory  {}\nfiles      {}\ndirs       {}\nbytes      {} ({})\ndepth      {}\nlargest    {} ({})\n",
        r.root_directory,
        r.total_files,
        r.total_directories,
        r.total_bytes,
        savehorse_core::format_bytes(r.total_bytes),
        r.max_directory_depth,
        r.largest_file_name,
        savehorse_core::format_bytes(r.largest_file_size)
    );

    Ok(Outcome::new(text, to_json(&r)?))
}

fn scout_diff(req: &Request) -> Result<Outcome> {
    let a = PathBuf::from(req.position(0, "a")?);
    let b = PathBuf::from(req.position(1, "b")?);

    let r = savehorse_scout::diff_files(&a, &b)?;

    let text = if r.is_identical {
        format!("identical — {} byte(s) each\n", r.size_a)
    } else {
        format!(
            "different\n  a        {} ({} byte(s))\n  b        {} ({} byte(s))\n  first at 0x{:X}\n  {} byte(s) differ\n",
            r.file_a,
            r.size_a,
            r.file_b,
            r.size_b,
            r.first_difference_offset.unwrap_or(0),
            r.differing_bytes_count
        )
    };

    Ok(Outcome::new(text, to_json(&r)?))
}


// ---------------------------------------------------------------------------
// aegis autostart, and flux
// ---------------------------------------------------------------------------

fn aegis_persistence(_req: &Request) -> Result<Outcome> {
    let r = savehorse_aegis::audit_autostart()?;

    let mut text = format!(
        "platform   {}\nlocations  {} checked, {} present\nentries    {}\n",
        r.platform,
        r.locations_checked.len(),
        r.locations_present,
        r.entries.len()
    );
    for entry in &r.entries {
        text.push_str(&format!(
            "  {:<28} {}{}\n",
            entry.name,
            entry.target.clone().unwrap_or_else(|| "(no target stated)".to_string()),
            if entry.target_exists { "" } else { "  [missing]" }
        ));
    }
    if r.entries.is_empty() {
        text.push_str("no autostart entries found\n");
    }
    text.push_str("\nfile-based autostart locations only; the Windows registry is not read\n");

    Ok(Outcome::new(text, to_json(&r)?))
}

fn flux_inspect(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let m = savehorse_flux::inspect(&path)?;

    let mut text = format!(
        "file       {}\nformat     {}\n",
        m.file_path, m.container_format
    );
    if let Some(channels) = m.channels {
        text.push_str(&format!(
            "channels   {}\nrate       {} Hz\nbits       {}\nduration   {} s\n",
            channels,
            m.sample_rate.unwrap_or(0),
            m.bits_per_sample.unwrap_or(0),
            m.duration_seconds.clone().unwrap_or_default()
        ));
    }
    text.push_str(&format!(
        "title      {}\nartist     {}\nalbum      {}\nyear       {}\n",
        m.title.clone().unwrap_or_else(|| "-".to_string()),
        m.artist.clone().unwrap_or_else(|| "-".to_string()),
        m.album.clone().unwrap_or_else(|| "-".to_string()),
        m.year.clone().unwrap_or_else(|| "-".to_string())
    ));

    Ok(Outcome::new(text, to_json(&m)?))
}

fn flux_synth(req: &Request) -> Result<Outcome> {
    let out = PathBuf::from(required_value(req, "out")?);
    let frequency = req.number("frequency")?.map(|v| v as f64).unwrap_or(440.0);
    let seconds = req.number("seconds")?.map(|v| v as f64).unwrap_or(1.0);
    let rate = req.number("rate")?.unwrap_or(44100) as u32;
    let channels = req.number("channels")?.unwrap_or(1) as u16;
    let volume = req.number("volume")?.map(|v| v as f64).unwrap_or(0.5);

    let r = savehorse_flux::synthesize(&out, frequency, seconds, rate, channels, volume)?;

    Ok(Outcome::new(
        format!(
            "wrote '{}' — {} Hz for {} s at {} Hz, {} channel(s), {} sample(s), {} byte(s)\n",
            r.output_file,
            r.frequency_hz,
            r.duration_seconds,
            r.sample_rate,
            r.channels,
            r.total_samples,
            r.file_size_bytes
        ),
        to_json(&r)?,
    ))
}

fn flux_tag(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let title = req.value("title").unwrap_or("");
    let artist = req.value("artist").unwrap_or("");
    let album = req.value("album").unwrap_or("");
    let year = req.value("year").unwrap_or("");

    let r = savehorse_flux::write_tag(&path, title, artist, album, year)?;

    Ok(Outcome::new(
        format!(
            "tagged '{}' — title '{}', artist '{}', album '{}', year '{}'{} ({} byte(s))\n",
            r.file,
            r.title,
            r.artist,
            r.album,
            r.year,
            if r.replaced_existing { " (replaced an existing tag)" } else { "" },
            r.file_size_bytes
        ),
        to_json(&r)?,
    ))
}

fn flux_gain(req: &Request) -> Result<Outcome> {
    let source = PathBuf::from(req.position(0, "path")?);
    let out = PathBuf::from(required_value(req, "out")?);
    let gain = req
        .number("gain")?
        .map(|v| v as f64)
        .ok_or_else(|| Error::Invalid("--gain is required, e.g. 1.5".to_string()))?;

    let r = savehorse_flux::apply_gain(&source, &out, gain)?;

    Ok(Outcome::new(
        format!(
            "wrote '{}' — {} sample(s) scaled by {}, {} clipped\n",
            r.destination_file, r.samples_processed, r.gain_multiplier, r.samples_clipped
        ),
        to_json(&r)?,
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("savehorse-runner-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn planned_operations_refuse_to_run() {
        // pulse.services is declared but still needs a platform API, so it is
        // the operation that refuses. (aegis.shred used to hold this role
        // until it was ported.)
        let req = Request::new("pulse", "services");
        let err = execute(&req).unwrap_err().to_string();
        assert!(err.contains("not ported yet"), "got: {}", err);
    }

    #[test]
    fn unknown_operations_name_themselves() {
        let req = Request::new("sift", "teleport");
        let err = execute(&req).unwrap_err().to_string();
        assert!(err.contains("no operation 'teleport'"), "got: {}", err);
    }

    #[test]
    fn search_returns_text_and_json() {
        let dir = scratch("search");
        let file = dir.join("a.txt");
        fs::write(&file, "needle here\nnothing\n").unwrap();

        let req = Request::new("sift", "search")
            .arg("needle")
            .arg(file.to_string_lossy().to_string());
        let out = execute(&req).unwrap();

        assert!(out.text.contains("1 match(es)"), "got: {}", out.text);
        assert!(out.json.contains("needle here"));
        assert!(!out.dry_run);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn replace_marks_itself_as_a_preview() {
        let dir = scratch("replace");
        let file = dir.join("a.txt");
        fs::write(&file, "cat").unwrap();

        let req = Request::new("sift", "replace")
            .arg("cat")
            .arg("dog")
            .arg(file.to_string_lossy().to_string())
            .flag("dry-run", "true");
        let out = execute(&req).unwrap();

        assert!(out.dry_run, "a dry run must report itself as a preview");
        assert_eq!(fs::read_to_string(&file).unwrap(), "cat");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn clean_without_commit_is_a_preview() {
        let dir = scratch("clean");
        fs::write(dir.join("a.txt"), b"same").unwrap();
        fs::write(dir.join("b.txt"), b"same").unwrap();

        let req = Request::new("scout", "clean").arg(dir.to_string_lossy().to_string());
        let out = execute(&req).unwrap();

        assert!(out.dry_run);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn catalog_renders_the_requested_format() {
        let dir = scratch("catalog");
        fs::write(dir.join("a.txt"), b"x").unwrap();

        let req = Request::new("scout", "catalog")
            .arg(dir.to_string_lossy().to_string())
            .flag("format", "json");
        let out = execute(&req).unwrap();

        let parsed: Vec<savehorse_scout::CatalogEntry> = serde_json::from_str(&out.text).unwrap();
        assert_eq!(parsed.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
