package main

import (
	"os"

	"github.com/urfave/cli/v2"
)

func main() {
	srcmgr().Run(os.Args)
}

var Version string = "0.0.1"

func srcmgr() *cli.App {
	app := cli.NewApp()
	app.Name = "srcmgr"
	app.Usage = "CVS repository manager"
	app.Version = Version
	app.Commands = Commands
	app.Authors = []*cli.Author{{Name: "KanchiShimono", Email: "dev.kanchi.shimono@gmail.com"}}
	return app
}
