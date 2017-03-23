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

func GetLocalRepository(fullPath string, rootPath string) (path *LocalRepository, err error) {
	relPath, _ := filepath.Rel(rootPath, fullPath)

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

func localRepositoryRoots() (roots []string) {
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

	roots = strings.Split(strings.TrimRight(string(buf), "\000"), "\000")

	if len(roots) == 0 {
		srcRoot := os.Getenv("GOPATH")
		if srcRoot != "" {
			srcRoot = filepath.Join(srcRoot, "src")
			roots = filepath.SplitList(srcRoot)
		} else {
			err := errors.New("GOPATH in not difined")
			panic(err)
		}
	}

	for i, v := range roots {
		path := filepath.Clean(v)
		if _, err := os.Stat(path); err == nil {
			roots[i], err = filepath.EvalSymlinks(path)
			if err != nil {
				panic(err)
			}
		} else {
			roots[i] = path
		}
	}

	return roots
}

func firstLocalRepositoryRoot() (root string) {
	return localRepositoryRoots()[0]
}
