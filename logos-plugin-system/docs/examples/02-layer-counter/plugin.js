// Layer Counter Plugin for Logos
// ==============================
//
// Shows real-time document statistics:
// - Total layer count
// - Count by type (Rectangle, Text, Frame, etc.)
// - Currently selected count
// - Buttons to select layers by type
//
// Demonstrates: getLayers, getSelection, setSelection, events, groups

// ─── Helper: Count layers by type ───
function countByType(layers) {
  var counts = {
    Rectangle: 0,
    Text: 0,
    Frame: 0,
    Component: 0,
    Group: 0
  };
  for (var i = 0; i < layers.length; i++) {
    var type = layers[i].type;
    if (counts[type] !== undefined) {
      counts[type] = counts[type] + 1;
    }
  }
  return counts;
}

// ─── Helper: Build component tree ───
function buildComponents(layers, selectedCount) {
  var counts = countByType(layers);

  return [
    { type: "group", label: "Document Stats", collapsed: false, children: [
      { type: "label", text: "Total: " + layers.length },
      { type: "label", text: "Selected: " + selectedCount }
    ]},
    { type: "separator" },
    { type: "group", label: "By Type", collapsed: false, children: [
      { type: "label", text: "Rectangles: " + counts.Rectangle },
      { type: "label", text: "Text: " + counts.Text },
      { type: "label", text: "Frames: " + counts.Frame },
      { type: "label", text: "Components: " + counts.Component },
      { type: "label", text: "Groups: " + counts.Group }
    ]},
    { type: "separator" },
    { type: "button", label: "Select All Rectangles", action: "select_rects" },
    { type: "button", label: "Select All Text", action: "select_text" },
    { type: "button", label: "Clear Selection", action: "clear" },
    { type: "button", label: "Refresh", action: "refresh" }
  ];
}

// ─── Initialize ───
var layers = Logos.getLayers();
var selection = Logos.getSelection();
var panelId = Logos.ui.createPanel("Layer Counter", "right", {
  components: buildComponents(layers, selection.length)
});

Logos.log("Layer Counter initialized: " + layers.length + " layers");

// ─── Update on selection change ───
Logos.on("selectionChanged", function(data) {
  var currentLayers = Logos.getLayers();
  Logos.ui.updatePanel(panelId, buildComponents(currentLayers, data.layerIds.length));
});

// ─── Update on layer added ───
Logos.on("layerAdded", function(data) {
  var currentLayers = Logos.getLayers();
  var sel = Logos.getSelection();
  Logos.ui.updatePanel(panelId, buildComponents(currentLayers, sel.length));
  Logos.log("Layer added: " + data.layerId + " (total: " + currentLayers.length + ")");
});

// ─── Update on layer removed ───
Logos.on("layerRemoved", function(data) {
  var currentLayers = Logos.getLayers();
  var sel = Logos.getSelection();
  Logos.ui.updatePanel(panelId, buildComponents(currentLayers, sel.length));
  Logos.log("Layer removed: " + data.layerId + " (total: " + currentLayers.length + ")");
});
