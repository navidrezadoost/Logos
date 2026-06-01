package filedata

import (
	"encoding/json"
	"log"
)

// LogShapeHitTest logs one line with fields relevant to click vs marquee selection.
func LogShapeHitTest(event, fileID, shapeID string, shape map[string]any) {
	if shape == nil {
		return
	}
	selrect, _ := json.Marshal(shape["selrect"])
	transform, _ := json.Marshal(shape["transform"])
	inv, _ := json.Marshal(shape["transform-inverse"])
	points, _ := json.Marshal(shape["points"])
	log.Printf("[shape-hit-test] event=%s file=%s id=%s type=%v name=%v blocked=%v hidden=%v locked=%v opacity=%v selrect=%s transform=%s transform-inverse=%s points=%s",
		event, fileID, shapeID,
		shape["type"], shape["name"],
		shape["blocked"], shape["hidden"], shape["locked"], shape["opacity"],
		string(selrect), string(transform), string(inv), string(points))
}
