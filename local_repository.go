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

func LocalRepositoryPath(fullPath string) (*LocalRepository, error) {
	srcRoot := localRepositoryRoot()

	relPath, err := filepath.Rel(srcRoot, fullPath)
	if err == nil {

	}

	if relPath == "" {
		return nil, fmt.Errorf("No local repository: %s", fullPath)
	}

	pathParts := strings.Split(relPath, string(filepath.Separator))

	return &LocalRepository{fullPath, relPath, pathParts}, nil
}

func localRepositoryRoots() []string {
	var _localRepositoryRoots []string

	srcRoot := os.Getenv("GOPATH")
	if srcRoot != "" {
		srcRoot = filepath.Join(srcRoot, "src")
		_localRepositoryRoots = filepath.SplitList(srcRoot)
	} else {
		err := errors.New("GOPATH in not difined")
		panic(err)
	}

	return _localRepositoryRoots
}

func localRepositoryRoot() string {
	return localRepositoryRoots()[0]
}
