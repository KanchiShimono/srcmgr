package main

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

type LocalRepository struct {
	FullPath  string
	RelPath   string
	PathParts []string
}

func LocalRepositoryPath(fullPath string) (path *LocalRepository, err error) {
	srcRoot := localRepositoryRoot()

	relPath, err := filepath.Rel(srcRoot, fullPath)
	if err == nil {

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

func localRepositoryRoots() (roots []string) {

	srcRoot := os.Getenv("GOPATH")
	if srcRoot != "" {
		srcRoot = filepath.Join(srcRoot, "src")
		roots = filepath.SplitList(srcRoot)
	} else {
		err := errors.New("GOPATH in not difined")
		panic(err)
	}

	return roots
}

func localRepositoryRoot() (root string) {
	return localRepositoryRoots()[0]
}
