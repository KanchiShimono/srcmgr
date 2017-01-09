package main

import (
	"os"

	"github.com/urfave/cli"
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
	app.Author = "KanchiShimono"
	app.Email = "shimono-kanchi-yc@ynu.jp"
	return app
}
