package main

import (
	"context"
	"fmt"
	"log"
	"net/http"

	"github.com/99designs/gqlgen/graphql"
	"github.com/99designs/gqlgen/graphql/handler"
	"github.com/99designs/gqlgen/graphql/handler/extension"
	"github.com/99designs/gqlgen/graphql/handler/transport"
	"github.com/99designs/gqlgen/graphql/playground"
	starwars "github.com/wasmerio/shipit/examples/go-graphql"
	"github.com/wasmerio/shipit/examples/go-graphql/generated"
)

func main() {
	srv := handler.New(generated.NewExecutableSchema(starwars.NewResolver()))
	srv.AddTransport(transport.GET{})
	srv.AddTransport(transport.POST{})
	srv.Use(extension.Introspection{})

	srv.AroundFields(func(ctx context.Context, next graphql.Resolver) (res any, err error) {
		rc := graphql.GetFieldContext(ctx)
		fmt.Println("Entered", rc.Object, rc.Field.Name)
		res, err = next(ctx)
		fmt.Println("Left", rc.Object, rc.Field.Name, "=>", res, err)
		return res, err
	})

	http.Handle("/", playground.Handler("Starwars", "/query"))
	http.Handle("/query", srv)

	log.Fatal(http.ListenAndServe(":8080", nil))
}
