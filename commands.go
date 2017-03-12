package main

import (
	"fmt"
	"os"
	"os/exec"
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
	if err := exec.Command("git", "--version").Run(); err != nil {
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

	srcRoot := os.Getenv("GOPATH")
	if srcRoot == "" {
		return util.ShowNewError("GOPATH is not found")
	}

	dest, _ := homedir.Expand(strings.TrimSpace(c.Args().Get(1)))
	if dest == "" {
		dest = filepath.Join(srcRoot, "src/github.com", username)
	}

	if _, err := os.Stat(dest); err != nil {
		if err := os.MkdirAll(dest, 0755); err != nil {
			return util.ShowExistError(err.Error(), err)
		}
		fmt.Printf("mkdir: created directory '%v'\n", dest)
	}

	dest = filepath.Join(dest, reponame)

	if _, err := os.Stat(dest); err == nil {
		fmt.Printf("%v: already exists\n", dest)
		fmt.Println("Overwrite repository? (Y/n)")
		var ans string
		fmt.Scanln(&ans)
		switch ans {
		case "y", "Y", "yes", "Yes", "YES":
			fmt.Printf("Overwrite repository... %v\n", dest)
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

	fmt.Printf("Cloning into '%v'...\n", dest)
	if err := exec.Command("git", "clone", "https://github.com/"+username+"/"+reponame+".git", dest).Run(); err == nil {
		return nil
	} else {
		return util.ShowExistError("Can not clone", err)
	}

}

func List(c *cli.Context) error {
	printRelPath := c.Bool("rel-path")
	printDeepPath := c.Bool("deep-path")
	srcRoots := localRepositoryRoots()

	for _, srcRoot := range srcRoots {
		if srcRoot == "" {
			return util.ShowNewError("srcmgr root directory is not found")
		}

		// srcRoot = filepath.Join(srcRoot, "src")

		var localRepositories []*LocalRepository

		filepath.Walk(srcRoot, func(path string, info os.FileInfo, err error) error {
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

			repo, err := LocalRepositoryPath(path)
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
