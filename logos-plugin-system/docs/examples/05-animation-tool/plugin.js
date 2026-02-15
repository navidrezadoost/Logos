// Animation Tool Plugin for Logos
// ================================
//
// Creates generative patterns:
// 1. Circular arrangement — shapes arranged in a circle
// 2. Spiral pattern — shapes along a spiral path
// 3. Path-based shapes — uses createPath for curves
//
// Demonstrates: createRect, createPath, undo, math transforms,
//               event system, complex UI, programmatic design

// ─── Configuration ───
var shapeType = "rect";   // "rect" or "circle"
var shapeCount = 12;
var radius = 150;
var scaleFactor = 1.0;
var rotationStep = 30;
var spacing = 10;
var centerX = 400;
var centerY = 400;
var autoUpdate = false;
var totalGenerated = 0;
var lastPatternIds = [];

// ─── Math helpers ───
var PI = 3.14159265358979;

function sin(angle) {
  // Taylor series approximation for sin
  var x = angle % (2 * PI);
  if (x > PI) x = x - 2 * PI;
  if (x < -PI) x = x + 2 * PI;
  var x2 = x * x;
  var x3 = x * x2;
  var x5 = x3 * x2;
  var x7 = x5 * x2;
  return x - x3 / 6 + x5 / 120 - x7 / 5040;
}

function cos(angle) {
  return sin(angle + PI / 2);
}

// ─── Generate circular pattern ───
function generateCircle() {
  var ids = [];
  var angleStep = (2 * PI) / shapeCount;
  var shapeSize = 30 * scaleFactor;

  for (var i = 0; i < shapeCount; i++) {
    Logos.checkTimeout();

    var angle = i * angleStep;
    var x = centerX + radius * cos(angle) - shapeSize / 2;
    var y = centerY + radius * sin(angle) - shapeSize / 2;

    var id;
    if (shapeType === "circle") {
      // Create a circle using bezier path approximation
      var cx = x + shapeSize / 2;
      var cy = y + shapeSize / 2;
      var r = shapeSize / 2;
      var k = 0.5522847498;  // Bezier approximation constant

      id = Logos.createPath([
        { command: "moveTo", x: cx, y: cy - r },
        { command: "bezierTo",
          cp1x: cx + r * k, cp1y: cy - r,
          cp2x: cx + r,     cp2y: cy - r * k,
          x: cx + r,        y: cy },
        { command: "bezierTo",
          cp1x: cx + r,     cp1y: cy + r * k,
          cp2x: cx + r * k, cp2y: cy + r,
          x: cx,            y: cy + r },
        { command: "bezierTo",
          cp1x: cx - r * k, cp1y: cy + r,
          cp2x: cx - r,     cp2y: cy + r * k,
          x: cx - r,        y: cy },
        { command: "bezierTo",
          cp1x: cx - r,     cp1y: cy - r * k,
          cp2x: cx - r * k, cp2y: cy - r,
          x: cx,            y: cy - r },
        { command: "close" }
      ]);
    } else {
      id = Logos.createRect(x, y, shapeSize, shapeSize);
    }

    ids.push(id);
    totalGenerated = totalGenerated + 1;
  }

  lastPatternIds = ids;
  Logos.setSelection(ids);

  Logos.log("Generated circle: " + ids.length + " shapes, radius=" + radius);
  Logos.ui.sendMessage(panelId, {
    type: "showNotification",
    text: "Created " + ids.length + " shapes in a circle"
  });

  return ids;
}

// ─── Generate spiral pattern ───
function generateSpiral() {
  var ids = [];
  var totalTurns = 3;
  var angleStep = (2 * PI * totalTurns) / shapeCount;
  var shapeSize = 20 * scaleFactor;
  var radiusStep = radius / shapeCount;

  for (var i = 0; i < shapeCount; i++) {
    Logos.checkTimeout();

    var angle = i * angleStep;
    var currentRadius = radiusStep * (i + 1);
    var x = centerX + currentRadius * cos(angle) - shapeSize / 2;
    var y = centerY + currentRadius * sin(angle) - shapeSize / 2;

    // Gradually increase size along the spiral
    var scale = 0.5 + (i / shapeCount) * 1.5;
    var size = shapeSize * scale;

    var id = Logos.createRect(x, y, size, size);
    ids.push(id);
    totalGenerated = totalGenerated + 1;
  }

  lastPatternIds = ids;
  Logos.setSelection(ids);

  Logos.log("Generated spiral: " + ids.length + " shapes");
  Logos.ui.sendMessage(panelId, {
    type: "showNotification",
    text: "Created spiral with " + ids.length + " shapes"
  });

  return ids;
}

// ─── Undo last pattern ───
function undoPattern() {
  if (lastPatternIds.length === 0) {
    Logos.log("Nothing to undo");
    return;
  }

  for (var i = lastPatternIds.length - 1; i >= 0; i--) {
    Logos.undo();
  }

  var undoneCount = lastPatternIds.length;
  lastPatternIds = [];
  totalGenerated = totalGenerated - undoneCount;

  Logos.log("Undone " + undoneCount + " shapes");
  Logos.ui.sendMessage(panelId, {
    type: "showNotification",
    text: "Undone " + undoneCount + " shapes"
  });
}

// ─── Create the UI panel ───
var panelId = Logos.ui.createPanel("Animation Tool", "right", {
  components: [
    { type: "group", label: "Pattern Settings", collapsed: false, children: [
      { type: "select", label: "Shape", key: "shape_type",
        value: "rect", options: ["rect", "circle"] },
      { type: "numberInput", label: "Count", key: "count",
        value: shapeCount, min: 3, max: 64, step: 1 },
      { type: "numberInput", label: "Radius", key: "radius",
        value: radius, min: 20, max: 500, step: 10 }
    ]},
    { type: "separator" },
    { type: "group", label: "Transform", collapsed: false, children: [
      { type: "numberInput", label: "Scale", key: "scale",
        value: 1.0, min: 0.1, max: 5.0, step: 0.1 },
      { type: "numberInput", label: "Rotation", key: "rotation",
        value: rotationStep, min: 0, max: 360, step: 5 },
      { type: "numberInput", label: "Spacing", key: "spacing",
        value: spacing, min: 0, max: 100, step: 1 }
    ]},
    { type: "separator" },
    { type: "group", label: "Center", collapsed: false, children: [
      { type: "numberInput", label: "X", key: "center_x",
        value: centerX, min: 0, max: 2000, step: 10 },
      { type: "numberInput", label: "Y", key: "center_y",
        value: centerY, min: 0, max: 2000, step: 10 }
    ]},
    { type: "separator" },
    { type: "toggle", label: "Auto-update preview", key: "auto_update",
      value: false },
    { type: "separator" },
    { type: "button", label: "Generate Circle Pattern", action: "circle" },
    { type: "button", label: "Generate Spiral Pattern", action: "spiral" },
    { type: "button", label: "Undo Last Pattern", action: "undo_pattern" },
    { type: "separator" },
    { type: "label", text: "Generated: " + totalGenerated + " shapes" }
  ]
});

// ─── Event listeners ───
Logos.on("layerAdded", function(data) {
  // Track created layers for potential undo
  Logos.log("Pattern shape created: " + data.layerId);
});

Logos.on("selectionChanged", function(data) {
  Logos.log("Selection changed: " + data.layerIds.length + " layers");
});

Logos.log("Animation Tool loaded — ready to create patterns!");
