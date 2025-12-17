package main

import (
	"fmt"
	"net/http"
)

func index(w http.ResponseWriter, req *http.Request) {
	fmt.Fprintf(w, "hello\n\nHeaders:\n")
	for name, headers := range req.Header {
		for _, h := range headers {
			fmt.Fprintf(w, " - %v: %v\n", name, h)
		}
	}
}

func main() {
	fmt.Println("Starting server on port 8080")
	http.HandleFunc("/", index)

	http.ListenAndServe(":8080", nil)
}
