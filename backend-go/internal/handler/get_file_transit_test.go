package handler

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/logos-design/logos/backend-go/internal/filedata"
	"github.com/logos-design/logos/backend-go/internal/transit"
)

func TestGetFileDataPagesIndexTransitUUIDKeys(t *testing.T) {
	pageID := "f177fa33-591f-4651-9687-8ec4c805fffc"
	fileID := "2f709b19-9811-4462-ab0e-ddc67949a1f5"
	data := filedata.BuildEmptyData(fileID, pageID)

	detail := &FileDetail{
		ID:   fileID,
		Data: data,
	}
	j, err := json.Marshal(detail)
	if err != nil {
		t.Fatal(err)
	}
	out, err := transit.JSONToTransit(j)
	if err != nil {
		t.Fatal(err)
	}
	s := string(out)
	if !strings.Contains(s, "~u"+pageID) {
		t.Fatalf("expected ~u page id in get-file transit payload:\n%s", s)
	}
}
