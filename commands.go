package main

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
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

	remoteRepo := c.Args().Get(0)

	// Check URL format
	if isValid := regexp.MustCompile(`^(((https?|git)://)?github\.com/)?([A-Za-z0-9_-]+/)?[A-Za-z0-9_.-]+(\.git)?$`).Match([]byte(remoteRepo)); !isValid {
		return util.ShowNewError("Invalid github.com URL")
	}

	// Format username/reponame
	repl1 := regexp.MustCompile(`^(((https?|git):\/\/)?github\.com\/)?`)
	repl2 := regexp.MustCompile(`\.git$`)
	uri := repl1.ReplaceAllString(remoteRepo, "")
	uri = repl2.ReplaceAllString(uri, "")
	// if uri has reponame only
	if hasUserName := regexp.MustCompile(`/`).Match([]byte(uri)); !hasUserName {
		user, err := exec.Command("git", "config", "--get", "user.name").Output()
		if err != nil {
			return util.ShowExistError("Git user name has not been set", err)
		}
		uri = strings.TrimSpace(string(user)) + "/" + uri
	}

	username := strings.Split(uri, "/")[0]
	reponame := strings.Split(uri, "/")[1]

	srcRoot := os.Getenv("GOPATH")
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
	if err := exec.Command("git", "clone", "https://github.com/"+uri+".git", dest).Run(); err == nil {
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
