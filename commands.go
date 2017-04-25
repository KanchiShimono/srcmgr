package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/KanchiShimono/srcmgr/util"
	"github.com/mitchellh/go-homedir"
	"github.com/urfave/cli"
)

var Commands = []cli.Command{
	commandGet,
	commandList,
}

var commandGet = cli.Command{
	Name:   "get",
	Usage:  "Clone git repository",
	Action: Get,
}

var commandList = cli.Command{
	Name:   "list",
	Usage:  "List managed repository",
	Action: List,
	Flags: []cli.Flag{
		cli.BoolFlag{Name: "rel-path, r", Usage: "Print relative path"},
		cli.BoolFlag{Name: "deep-path, d", Usage: "Print path to .git of sub directories"},
	},
}

func Get(c *cli.Context) error {
	// Chech to have git command
	if err := util.RunSilent("git", "--version"); err != nil {
		return util.ShowExistError("You don't have git command", err)
	}

	remoteRepo, err := NewRemoteRepository(c.Args().Get(0))
	if err != nil {
		return util.ShowExistError(err.Error(), err)
	}

	// Check URL format
	if !remoteRepo.IsValid() {
		return util.ShowNewError("Invalid github.com URL")
	}

	// Format username/reponame
	names := remoteRepo.Format4UsrRepoNames()
	username, reponame := remoteRepo.UsrRepoNameFrom(names)

	srcRoot := firstLocalRepositoryRoot()
	if srcRoot == "" {
		return util.ShowNewError("srcmgr root directory is not found")
	}

	dest, _ := homedir.Expand(strings.TrimSpace(c.Args().Get(1)))
	destArgExist := true
	if dest == "" {
		dest = filepath.Join(srcRoot, remoteRepo.URL().Hostname(), username)
		destArgExist = false
	}

	if !destArgExist {
		dest = filepath.Join(dest, reponame)
	}

	// If dest is below "srcRoot", user can chose overwrite repository
	if _, err := os.Stat(dest); err == nil && strings.HasPrefix(dest, srcRoot) {
		fmt.Printf("%v: already exists\n", dest)
		fmt.Println("Overwrite repository? (Y/n)")
		var ans string
		fmt.Scanln(&ans)
		switch ans {
		case "y", "Y", "yes", "Yes", "YES":
			fmt.Printf("Removing repository... %v\n", dest)
			os.RemoveAll(dest)
		case "n", "N", "no", "No", "NO":
			return nil
		default:
			return util.ShowNewError("Invalid input")
		}
	}

	if err := os.MkdirAll(dest, 0755); err != nil {
		return util.ShowExistError(err.Error(), err)
	}

	if err := util.Run("git", "clone", "https://github.com/"+username+"/"+reponame+".git", dest); err == nil {
		return nil
	} else {
		return util.ShowExistError("Can not clone", err)
	}

}

func List(c *cli.Context) error {
	printRelPath := c.Bool("rel-path")
	printDeepPath := c.Bool("deep-path")
	rootPaths := getLocalRepositoryRoots()

	for _, rootPath := range rootPaths {
		if rootPath == "" {
			return util.ShowNewError("srcmgr root directory is not found")
		}

		var localRepositories []*LocalRepository

		filepath.Walk(rootPath, func(path string, info os.FileInfo, err error) error {
			if info == nil || info.IsDir() == false || err != nil {
				return nil
			}

			existVCSDir := false
			var VCSDIR = []string{".git", ".hg", ".svn"}

			for _, vcs := range VCSDIR {
				file, err := os.Stat(filepath.Join(path, vcs))
				if err == nil && file.IsDir() {
					existVCSDir = true
					break
				}
			}

			if !existVCSDir {
				return nil
			}

			repo, err := GetLocalRepository(path)
			if err != nil || repo == nil {
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
	}

	return nil
}
