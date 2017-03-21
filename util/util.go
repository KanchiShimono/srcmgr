package util

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
)

func ShowExistError(msg string, err error) error {
	fmt.Println(msg)
	return err
}

func ShowNewError(msg string) error {
	err := errors.New(msg)
	fmt.Println(err)
	return err
}

func Run(cmd string, opts ...string) (err error) {
	c := exec.Command(cmd, opts...)
	c.Stdout = os.Stdout
	c.Stderr = os.Stderr
	return c.Run()
}
