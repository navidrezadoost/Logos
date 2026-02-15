// Hello World Plugin for Logos
// ===========================
//
// The simplest possible plugin:
// - Creates a UI panel with document info
// - Adds buttons for common actions
// - Listens for selection changes
//
// Permissions required: document.read, ui.panel

// 1. Read document info
const info = Logos.getDocumentInfo();
Logos.log("Hello World plugin loaded!");
Logos.log("Document: " + info.pageName + " (" + info.layerCount + " layers)");

// 2. Create a panel with buttons
const panelId = Logos.ui.createPanel("Hello World", "right", {
  components: [
    { type: "label", text: "👋 Hello from Logos!" },
    { type: "separator" },
    { type: "label", text: "Document: " + info.pageName },
    { type: "label", text: "Layers: " + info.layerCount },
    { type: "separator" },
    { type: "button", label: "Count Layers", action: "count" },
    { type: "button", label: "Create Rectangle", action: "create_rect" },
    { type: "button", label: "Show Selection", action: "show_selection" }
  ]
});

// 3. Listen for selection changes
Logos.on("selectionChanged", function(data) {
  var count = data.layerIds.length;
  var msg = count + " layer" + (count !== 1 ? "s" : "") + " selected";
  Logos.ui.sendMessage(panelId, {
    type: "showNotification",
    text: msg
  });
});

// 4. Listen for new layers being added
Logos.on("layerAdded", function(data) {
  var newCount = Logos.getLayerCount();
  Logos.log("Layer added! Total layers: " + newCount);

  // Update the layer count label
  Logos.ui.sendMessage(panelId, {
    type: "updateValue",
    key: "layer_count",
    value: "Layers: " + newCount
  });
});

Logos.log("Hello World panel created (id: " + panelId + ")");
