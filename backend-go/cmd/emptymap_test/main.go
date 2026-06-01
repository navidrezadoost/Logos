package main

import (
	"fmt"
	"github.com/logos-design/logos/backend-go/internal/transit"
)

func main() {
	b, _ := transit.JSONToTransit([]byte("{}"))
	fmt.Printf("empty map: %q len=%d\n", string(b), len(b))
}
