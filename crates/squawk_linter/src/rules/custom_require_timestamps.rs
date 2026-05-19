use squawk_syntax::{
    Parse, SourceFile,
    ast::{self, AstNode},
};

use crate::{Linter, Rule, Violation};

pub(crate) fn custom_require_timestamps(ctx: &mut Linter, parse: &Parse<SourceFile>) {
    for stmt in parse.tree().stmts() {
        if let ast::Stmt::CreateTable(create_table) = stmt {
            check_create_table(ctx, create_table);
        }
    }
}

fn check_create_table(ctx: &mut Linter, create_table: ast::CreateTable) {
    let columns = create_table
        .table_arg_list()
        .into_iter()
        .flat_map(|args| args.args())
        .filter_map(|arg| match arg {
            ast::TableArg::Column(column) => Some(column),
            _ => None,
        })
        .collect::<Vec<_>>();

    check_timestamp_column(
        ctx,
        create_table.syntax(),
        &columns,
        "created_at",
        TimestampColumnKind::RequiredWithDefault,
    );
    check_timestamp_column(
        ctx,
        create_table.syntax(),
        &columns,
        "updated_at",
        TimestampColumnKind::RequiredWithDefault,
    );
    check_timestamp_column(
        ctx,
        create_table.syntax(),
        &columns,
        "deleted_at",
        TimestampColumnKind::NullableNoDefault,
    );
}

#[derive(Clone, Copy)]
enum TimestampColumnKind {
    RequiredWithDefault,
    NullableNoDefault,
}

fn check_timestamp_column(
    ctx: &mut Linter,
    create_table: &squawk_syntax::SyntaxNode,
    columns: &[ast::Column],
    name: &str,
    kind: TimestampColumnKind,
) {
    let Some(column) = columns.iter().find(|column| {
        column
            .name()
            .is_some_and(|column_name| column_name.text() == name)
    }) else {
        ctx.report(
            Violation::for_node(
                Rule::CustomRequireTimestamps,
                format!("Table is missing required `{name}` column."),
                create_table,
            )
            .help(format!("Add `{}`.", expected_column_definition(name, kind))),
        );
        return;
    };

    let valid = match kind {
        TimestampColumnKind::RequiredWithDefault => {
            is_timestamptz(column.ty().as_ref()) && has_not_null(column) && has_default_now(column)
        }
        TimestampColumnKind::NullableNoDefault => {
            is_timestamptz(column.ty().as_ref()) && !has_not_null(column) && !has_default(column)
        }
    };

    if !valid {
        let expected = match kind {
            TimestampColumnKind::RequiredWithDefault => {
                format!("`{name}` must be `timestamptz NOT NULL DEFAULT now()`.")
            }
            TimestampColumnKind::NullableNoDefault => {
                format!("`{name}` must be nullable `timestamptz` with no default.")
            }
        };
        ctx.report(Violation::for_node(
            Rule::CustomRequireTimestamps,
            expected,
            column.syntax(),
        ));
    }
}

fn expected_column_definition(name: &str, kind: TimestampColumnKind) -> String {
    match kind {
        TimestampColumnKind::RequiredWithDefault => {
            format!("{name} timestamptz NOT NULL DEFAULT now()")
        }
        TimestampColumnKind::NullableNoDefault => format!("{name} timestamptz"),
    }
}

fn is_timestamptz(ty: Option<&ast::Type>) -> bool {
    match ty {
        Some(ast::Type::PathType(path_type)) => path_type
            .path()
            .and_then(|path| path.segment())
            .and_then(|segment| segment.name_ref())
            .is_some_and(|name| name.text().eq_ignore_ascii_case("timestamptz")),
        Some(ast::Type::TimeType(time_type)) => {
            time_type.timestamp_token().is_some()
                && matches!(time_type.timezone(), Some(ast::Timezone::WithTimezone(_)))
        }
        _ => false,
    }
}

fn has_not_null(column: &ast::Column) -> bool {
    column
        .constraints()
        .any(|constraint| matches!(constraint, ast::ColumnConstraint::NotNullConstraint(_)))
}

fn has_default(column: &ast::Column) -> bool {
    column
        .constraints()
        .any(|constraint| matches!(constraint, ast::ColumnConstraint::DefaultConstraint(_)))
}

fn has_default_now(column: &ast::Column) -> bool {
    column.constraints().any(|constraint| {
        let ast::ColumnConstraint::DefaultConstraint(default) = constraint else {
            return false;
        };
        default
            .expr()
            .is_some_and(|expr| compact_text(expr.syntax()).eq_ignore_ascii_case("now()"))
    })
}

fn compact_text(syntax: &squawk_syntax::SyntaxNode) -> String {
    syntax
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .map(|token| token.text().to_string())
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod test {
    use insta::assert_snapshot;

    use crate::{
        Rule,
        test_utils::{lint_errors, lint_ok},
    };

    #[test]
    fn err_missing_fields() {
        let sql = r#"
CREATE TABLE my_table (
  id int
);
"#;
        assert_snapshot!(lint_errors(sql, Rule::CustomRequireTimestamps));
    }

    #[test]
    fn err_wrong_type() {
        let sql = r#"
CREATE TABLE my_table (
  id int,
  created_at timestamp DEFAULT now(),
  updated_at timestamp DEFAULT now(),
  deleted_at timestamp
);
"#;
        assert_snapshot!(lint_errors(sql, Rule::CustomRequireTimestamps));
    }

    #[test]
    fn err_missing_not_null() {
        let sql = r#"
CREATE TABLE my_table (
  id int,
  created_at timestamptz DEFAULT now(),
  updated_at timestamptz DEFAULT now(),
  deleted_at timestamptz
);
"#;
        assert_snapshot!(lint_errors(sql, Rule::CustomRequireTimestamps));
    }

    #[test]
    fn err_missing_default() {
        let sql = r#"
CREATE TABLE my_table (
  id int,
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  deleted_at timestamptz
);
"#;
        assert_snapshot!(lint_errors(sql, Rule::CustomRequireTimestamps));
    }

    #[test]
    fn err_is_not_null() {
        let sql = r#"
CREATE TABLE my_table (
  id int,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz NOT NULL
);
"#;
        assert_snapshot!(lint_errors(sql, Rule::CustomRequireTimestamps));
    }

    #[test]
    fn err_has_default() {
        let sql = r#"
CREATE TABLE my_table (
  id int,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz DEFAULT now()
);
"#;
        assert_snapshot!(lint_errors(sql, Rule::CustomRequireTimestamps));
    }

    #[test]
    fn ok() {
        let sql = r#"
CREATE TABLE my_table (
  id int,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz
);
"#;
        lint_ok(sql, Rule::CustomRequireTimestamps);
    }
}
