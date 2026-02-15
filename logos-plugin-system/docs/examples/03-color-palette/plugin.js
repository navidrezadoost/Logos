// Color Palette Plugin for Logos
// ==============================
//
// Create colored rectangles with precise control over:
// - Fill color (via ColorPicker)
// - Size (Width × Height)
// - Position (X, Y)
// - Opacity
//
// Demonstrates: createRect, undo, colorPicker, numberInput, toggle, groups

// ─── Default values ───
var currentColor = { r: 66, g: 133, b: 244, a: 1.0 };  // Logos Blue
var rectWidth = 200;
var rectHeight = 200;
var posX = 100;
var posY = 100;
var opacity = 100;
var centerMode = true;
var createdCount = 0;

// ─── Create the panel ───
var panelId = Logos.ui.createPanel("Color Palette", "right", {
  components: [
    { type: "group", label: "Color", collapsed: false, children: [
      { type: "colorPicker", label: "Fill", key: "fill",
        value: currentColor },
      { type: "numberInput", label: "Opacity", key: "opacity",
        value: opacity, min: 0, max: 100, step: 1 }
    ]},
    { type: "separator" },
    { type: "group", label: "Size", collapsed: false, children: [
      { type: "numberInput", label: "Width", key: "width",
        value: rectWidth, min: 1, max: 2000, step: 10 },
      { type: "numberInput", label: "Height", key: "height",
        value: rectHeight, min: 1, max: 2000, step: 10 }
    ]},
    { type: "separator" },
    { type: "group", label: "Position", collapsed: false, children: [
      { type: "numberInput", label: "X", key: "pos_x",
        value: posX, min: -5000, max: 5000, step: 10 },
      { type: "numberInput", label: "Y", key: "pos_y",
        value: posY, min: -5000, max: 5000, step: 10 }
    ]},
    { type: "separator" },
    { type: "toggle", label: "Create at center", key: "center_mode",
      value: centerMode },
    { type: "separator" },
    { type: "button", label: "Create Shape", action: "create" },
    { type: "button", label: "Undo Last", action: "undo" },
    { type: "button", label: "Create Grid (3×3)", action: "grid" },
    { type: "separator" },
    { type: "label", text: "Shapes created: 0" }
  ]
});

Logos.log("Color Palette plugin loaded");

// ─── Create a single rectangle ───
function createColoredRect(x, y, w, h) {
  var id = Logos.createRect(x, y, w, h);
  createdCount = createdCount + 1;
  Logos.log("Created rect #" + createdCount + " at (" + x + ", " + y + ") " + w + "×" + h);

  // Update counter display
  Logos.ui.sendMessage(panelId, {
    type: "showNotification",
    text: "Created rectangle #" + createdCount
  });

  return id;
}

// ─── Create a 3×3 grid of rectangles ───
function createGrid() {
  var spacing = 10;
  var cellW = rectWidth;
  var cellH = rectHeight;
  var startX = posX;
  var startY = posY;

  for (var row = 0; row < 3; row++) {
    for (var col = 0; col < 3; col++) {
      var x = startX + col * (cellW + spacing);
      var y = startY + row * (cellH + spacing);
      createColoredRect(x, y, cellW, cellH);
    }
  }

  Logos.ui.sendMessage(panelId, {
    type: "showNotification",
    text: "Created 3×3 grid (9 rectangles)"
  });
}

// ─── Listen for layer creation ───
Logos.on("layerAdded", function(data) {
  Logos.log("New layer: " + data.layerId);
});
