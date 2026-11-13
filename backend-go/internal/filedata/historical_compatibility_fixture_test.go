package filedata

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func TestP0HistoricalFlexLayoutEnvelopeRemainsOpaqueOnRoundTrip(t *testing.T) {
	fixturePath := filepath.Join("..", "..", "..", "test", "fixtures", "p0-document-compat", "historical-flex-layouts.json")
	raw, err := os.ReadFile(fixturePath)
	if err != nil {
		t.Fatalf("read historical compatibility fixture: %v", err)
	}

	const sourceSHA256 = "8af0a6ced3bab8f4fc4b5d738d457477591ad1f3291710bd1f7e4596eed20b12"
	digest := sha256.Sum256(raw)
	if got := hex.EncodeToString(digest[:]); got != sourceSHA256 {
		t.Fatalf("historical fixture SHA-256 = %s, want verbatim source digest %s", got, sourceSHA256)
	}

	var document map[string]any
	if err := json.Unmarshal(raw, &document); err != nil {
		t.Fatalf("decode historical compatibility fixture: %v", err)
	}
	var original map[string]any
	if err := json.Unmarshal(raw, &original); err != nil {
		t.Fatalf("decode original historical compatibility fixture: %v", err)
	}

	features, ok := document["~:features"].(map[string]any)
	if !ok || !containsString(features["~#set"], "fdata/objects-map") {
		t.Fatalf("fixture no longer identifies the historical objects-map feature: %#v", document["~:features"])
	}
	data, ok := document["~:data"].(map[string]any)
	if !ok {
		t.Fatalf("historical data envelope missing: %#v", document["~:data"])
	}
	pages, ok := data["~:pages-index"].(map[string]any)
	if !ok || len(pages) == 0 {
		t.Fatalf("historical page index missing: %#v", data["~:pages-index"])
	}

	// In this historical response, the Transit objects-map stores each shape as
	// a serialized string. Check that the retained sample includes all four flex
	// directions without pretending this Go path can decode those shape values.
	directions := map[string]bool{
		"row":            false,
		"column":         false,
		"row-reverse":    false,
		"column-reverse": false,
	}
	shapeCount := 0
	for _, pageValue := range pages {
		page, ok := pageValue.(map[string]any)
		if !ok {
			t.Fatalf("historical page has unexpected representation: %T", pageValue)
		}
		objects, ok := page["~:objects"].(map[string]any)
		if !ok {
			t.Fatalf("historical objects map missing: %#v", page["~:objects"])
		}
		objectMap, ok := objects["~#penpot/objects-map/v2"].(map[string]any)
		if !ok {
			t.Fatalf("historical objects-map wrapper missing: %#v", page["~:objects"])
		}
		for _, shapeValue := range objectMap {
			shapeData, ok := shapeValue.(string)
			if !ok {
				t.Fatalf("historical shape value has unexpected representation: %T", shapeValue)
			}
			shapeCount++
			if !strings.Contains(shapeData, `"~:layout","~:flex"`) {
				continue
			}
			for direction := range directions {
				if strings.Contains(shapeData, `"~:layout-flex-dir","~:`+direction+`"`) {
					directions[direction] = true
				}
			}
		}
	}
	if shapeCount == 0 {
		t.Fatal("historical objects-map contained no serialized shape values")
	}
	for direction, found := range directions {
		if !found {
			t.Errorf("historical fixture no longer contains a flex shape with direction %q", direction)
		}
	}

	if NormalizeFileData(document) {
		t.Fatal("Go filedata normalizer unexpectedly claimed to normalize the historical Transit envelope")
	}
	if !reflect.DeepEqual(document, original) {
		t.Fatal("Go filedata normalizer mutated the unsupported historical envelope")
	}

	encoded, err := EncodeJSON(document)
	if err != nil {
		t.Fatalf("encode opaque historical envelope: %v", err)
	}
	var reopened map[string]any
	if err := json.Unmarshal(encoded, &reopened); err != nil {
		t.Fatalf("decode opaque historical envelope: %v", err)
	}
	if !reflect.DeepEqual(reopened, original) {
		t.Fatal("opaque historical envelope changed across Go JSON encode/decode")
	}
}

func containsString(value any, want string) bool {
	values, ok := value.([]any)
	if !ok {
		return false
	}
	for _, value := range values {
		if value == want {
			return true
		}
	}
	return false
}
