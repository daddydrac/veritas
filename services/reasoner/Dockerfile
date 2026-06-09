FROM eclipse-temurin:21-jre
ARG OPENLLET_VERSION=2.6.5
RUN apt-get update && apt-get install -y --no-install-recommends curl unzip ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /opt/openllet
RUN curl -fsSL "https://github.com/Galigator/openllet/releases/download/openllet-${OPENLLET_VERSION}/openllet-${OPENLLET_VERSION}.zip" -o /tmp/openllet.zip \
  && unzip /tmp/openllet.zip -d /opt \
  && mv /opt/openllet-${OPENLLET_VERSION}/* /opt/openllet/ \
  && rm -rf /tmp/openllet.zip /opt/openllet-${OPENLLET_VERSION}
ENV PATH="/opt/openllet/bin:${PATH}"
WORKDIR /workspace
ENTRYPOINT ["openllet"]
