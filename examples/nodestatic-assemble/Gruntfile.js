module.exports = function (grunt) {
  grunt.initConfig({
    assemble: {
      pages: {
        files: [
          {
            expand: true,
            cwd: "src",
            src: ["**/*.hbs"],
            dest: "dist/",
            ext: ".html",
          },
        ],
      },
    },
  });

  grunt.loadNpmTasks("grunt-assemble");
  grunt.registerTask("default", ["assemble"]);
};
