// Export Helper Plugin for Logos
// ==============================
//
// Generates structured reports of the document:
// - JSON layer tree
// - CSV layer listing
// - Plain text summary
//
// Demonstrates: getLayers, getSelection, getDocumentInfo,
//               select dropdown, toggle, document traversal

// ─── State ───
var exportFormat = "json";
var includeMetadata = true;
var includePositions = true;

// ─── Build summary stats ───
function buildSummary() {
  var info = Logos.getDocumentInfo();
  var layers = Logos.getLayers();
  var counts = {};

  for (var i = 0; i < layers.length; i++) {
    var t = layers[i].type;
    counts[t] = (counts[t] || 0) + 1;
  }

  return {
    name: info.pageName,
    total: layers.length,
    counts: counts,
    layers: layers
  };
}

// ─── Generate JSON report ───
function generateJSON(summary) {
  var report = {
    document: summary.name,
    generated: "Logos Export Helper v1.0.0",
    totalLayers: summary.total,
    layersByType: summary.counts
  };

  if (includePositions) {
    report.layers = [];
    for (var i = 0; i < summary.layers.length; i++) {
      var layer = summary.layers[i];
      var entry = {
        id: layer.id,
        name: layer.name,
        type: layer.type
      };
      if (includePositions) {
        entry.x = layer.x;
        entry.y = layer.y;
        entry.width = layer.width;
        entry.height = layer.height;
        entry.rotation = layer.rotation;
      }
      report.layers.push(entry);
    }
  }

  return JSON.stringify(report, null, 2);
}

// ─── Generate CSV report ───
function generateCSV(summary) {
  var lines = [];
  var header = "id,name,type";
  if (includePositions) {
    header = header + ",x,y,width,height,rotation";
  }
  lines.push(header);

  for (var i = 0; i < summary.layers.length; i++) {
    var l = summary.layers[i];
    var row = l.id + "," + l.name + "," + l.type;
    if (includePositions) {
      row = row + "," + l.x + "," + l.y + "," + l.width + "," + l.height + "," + l.rotation;
    }
    lines.push(row);
  }

  return lines.join("\n");
}

// ─── Generate plain text report ───
function generateText(summary) {
  var lines = [];
  lines.push("═══ Document Report ═══");
  lines.push("Document: " + summary.name);
  lines.push("Total Layers: " + summary.total);
  lines.push("");
  lines.push("── By Type ──");

  var types = Object.keys(summary.counts);
  for (var i = 0; i < types.length; i++) {
    lines.push("  " + types[i] + ": " + summary.counts[types[i]]);
  }

  lines.push("");
  lines.push("── Layers ──");

  for (var j = 0; j < summary.layers.length; j++) {
    var l = summary.layers[j];
    var line = "  [" + l.type + "] " + l.name;
    if (includePositions) {
      line = line + " @ (" + l.x + ", " + l.y + ") " + l.width + "×" + l.height;
    }
    lines.push(line);
  }

  return lines.join("\n");
}

// ─── Build the panel ───
var summary = buildSummary();

var panelId = Logos.ui.createPanel("Export Helper", "right", {
  components: [
    { type: "group", label: "Export Options", collapsed: false, children: [
      { type: "select", label: "Format", key: "format",
        value: "json",
        options: ["json", "csv", "text"] },
      { type: "toggle", label: "Include metadata", key: "metadata",
        value: true },
      { type: "toggle", label: "Include positions", key: "positions",
        value: true }
    ]},
    { type: "separator" },
    { type: "group", label: "Document Summary", collapsed: false, children: [
      { type: "label", text: "Name: " + summary.name },
      { type: "label", text: "Total layers: " + summary.total },
      { type: "label", text: "Rectangles: " + (summary.counts.Rectangle || 0) },
      { type: "label", text: "Text layers: " + (summary.counts.Text || 0) },
      { type: "label", text: "Frames: " + (summary.counts.Frame || 0) }
    ]},
    { type: "separator" },
    { type: "button", label: "Generate Report", action: "generate" },
    { type: "button", label: "Export Selection Only", action: "export_selection" },
    { type: "button", label: "Refresh Stats", action: "refresh" }
  ]
});

// ─── Handle actions ───
Logos.on("documentChanged", function() {
  // Auto-refresh stats when document changes
  var newSummary = buildSummary();
  Logos.log("Document changed: " + newSummary.total + " layers");
});

Logos.log("Export Helper loaded (" + summary.total + " layers)");
