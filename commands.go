package main

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/urfave/cli"
)

var Commands = []cli.Command{
	// commandGet,
	commandList,
}

// var commandGet = cli.Command{
// 	Name:   "get",
// 	Usage:  "Clone git repository",
// 	Action: Get,
// }

var commandList = cli.Command{
	Name:   "list",
	Usage:  "List managed repository",
	Action: List,
	Flags: []cli.Flag{
		cli.BoolFlag{Name: "rel-path, r", Usage: "Print relative path"},
		cli.BoolFlag{Name: "deep-path, d", Usage: "Print path to .git of sub directories"},
	},
}

// func Get(c *cli.Context) error {
// 	return nil
// }

func List(c *cli.Context) error {
	printRelPath := c.Bool("rel-path")
	printDeepPath := c.Bool("deep-path")
	srcRoot := os.Getenv("GOPATH")

	if srcRoot == "" {
		err := errors.New("GOPATH is not found")
		return err
	}

	srcRoot = filepath.Join(srcRoot, "src")

	var localRepositories []*LocalRepository

	filepath.Walk(srcRoot, func(path string, info os.FileInfo, err error) error {
		if info == nil || info.IsDir() == false || err != nil {
			return nil
		}

		existVCSDir := false
		var VCSDIR = []string{".git", ".hg", ".svn"}

		for _, vcs := range VCSDIR {
			_, err = os.Stat(filepath.Join(path, vcs))
			if err == nil {
				existVCSDir = true
				break
			}
		}

		if !existVCSDir {
			return nil
		}

		repo, err := LocalRepositoryPath(path)
		if err != nil {
			return nil
		}

		if repo == nil {
			return nil
		}

		localRepositories = append(localRepositories, repo)

		if printDeepPath {
			return nil
		}

		return filepath.SkipDir
	})

	for _, repo := range localRepositories {
		if printRelPath {
			fmt.Println(repo.RelPath)
		} else {
			fmt.Println(repo.FullPath)
		}
	}

	return nil
}
