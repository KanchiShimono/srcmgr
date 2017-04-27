package main

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

type LocalRepository struct {
	FullPath  string
	RelPath   string
	PathParts []string
}

func GetLocalRepository(fullPath string) (path *LocalRepository, err error) {
	var relPath string

	for _, rootPath := range getLocalRepositoryRoots() {
		if !strings.HasPrefix(fullPath, rootPath) {
			continue
		}

		relPath, err = filepath.Rel(rootPath, fullPath)
		if err == nil {
			break
		}
	}

	if relPath == "" {
		return nil, fmt.Errorf("No local repository: %s", fullPath)
	}

	pathParts := strings.Split(relPath, string(filepath.Separator))

	path = &LocalRepository{
		FullPath:  fullPath,
		RelPath:   relPath,
		PathParts: pathParts,
	}

	return path, nil
}

var localRepositoryRoots []string

func getLocalRepositoryRoots() (roots []string) {
	if len(localRepositoryRoots) != 0 {
		return localRepositoryRoots
	}

	buf, err := exec.Command(
		"git",
		"config",
		"--path",
		"--null",
		"--get-all",
		"srcmgr.root").Output()

	// TODO
	// I have to fix following source code to continue running if err =! nil.
	// Now program will stop when above git command return error.
	// However I have to accept the path execting srcmgr.root written in .gitconfig
	// I will fix ASAP with highest priority.
	if err != nil {
		fmt.Println("srcmgr root has not been set in .gitconfig")
		panic(err)
	}

	localRepositoryRoots = strings.Split(strings.TrimRight(string(buf), "\000"), "\000")

	if len(localRepositoryRoots) == 0 {
		srcRoot := os.Getenv("GOPATH")
		if srcRoot != "" {
			srcRoot = filepath.Join(srcRoot, "src")
			localRepositoryRoots = filepath.SplitList(srcRoot)
		} else {
			err := errors.New("GOPATH is not defined")
			panic(err)
		}
	}

	for i, v := range localRepositoryRoots {
		path := filepath.Clean(v)
		if _, err := os.Stat(path); err == nil {
			localRepositoryRoots[i], err = filepath.EvalSymlinks(path)
			if err != nil {
				panic(err)
			}
		} else {
			localRepositoryRoots[i] = path
		}
	}

	return localRepositoryRoots
}

func firstLocalRepositoryRoot() (root string) {
	return getLocalRepositoryRoots()[0]
}
